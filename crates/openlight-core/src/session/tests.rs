//! Pruebas de la sesión con dos focos falsos y una captura falsa con video sintético.
//! El reloj de tokio está pausado: `sleep` adelanta el tiempo virtual, así que las pruebas
//! de 20 segundos de "sin pantalla" corren al instante y sin depender de la máquina.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use openlight_types::{
    CaptureError, DeviceError, DeviceId, DeviceSessionState, DisplayId, DisplayInfo, Frame, Hsv,
    LinkHealth, PermissionState, ScreenSource, SessionState, Settings, Zone, ZonePreset,
};
use tokio::sync::watch;
use tokio::time::sleep;

use super::*;
use crate::pipeline::tests::{AZUL, ROJO, VERDE, display, halves, solid};

// ---------- Foco falso ----------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Ev {
    Prepare,
    Send(Hsv),
    Restore,
}

#[derive(Debug, Default)]
struct LightState {
    log: Vec<Ev>,
    connected: bool,
    reconnects: u32,
    fail_sends: bool,
    fail_prepare: Option<DeviceError>,
}

/// Lo que ve la prueba de un foco falso; el mismo estado lo comparte el `LightDriver`.
#[derive(Debug, Clone)]
struct FakeLight(Arc<Mutex<LightState>>);

impl FakeLight {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(LightState {
            connected: true,
            ..LightState::default()
        })))
    }

    fn driver(&self) -> Box<dyn LightDriver> {
        Box::new(FakeDriver(self.clone()))
    }

    fn log(&self) -> Vec<Ev> {
        self.0.lock().unwrap().log.clone()
    }

    fn sends(&self) -> Vec<Hsv> {
        self.log()
            .into_iter()
            .filter_map(|e| match e {
                Ev::Send(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    fn count(&self, wanted: Ev) -> usize {
        self.log()
            .iter()
            .filter(|e| std::mem::discriminant(*e) == std::mem::discriminant(&wanted))
            .count()
    }

    fn with(&self, change: impl FnOnce(&mut LightState)) {
        change(&mut self.0.lock().unwrap());
    }
}

struct FakeDriver(FakeLight);

#[async_trait]
impl LightDriver for FakeDriver {
    async fn prepare(&mut self) -> std::result::Result<(), DeviceError> {
        let mut s = self.0.0.lock().unwrap();
        s.log.push(Ev::Prepare);
        s.fail_prepare.clone().map_or(Ok(()), Err)
    }

    async fn send_hsv(&mut self, color: Hsv) -> std::result::Result<(), DeviceError> {
        let mut s = self.0.0.lock().unwrap();
        if s.fail_sends {
            return Err(DeviceError::Timeout);
        }
        s.log.push(Ev::Send(color));
        Ok(())
    }

    async fn restore(&mut self) -> std::result::Result<bool, DeviceError> {
        self.0.0.lock().unwrap().log.push(Ev::Restore);
        Ok(true)
    }

    fn health(&self) -> LinkHealth {
        let s = self.0.0.lock().unwrap();
        LinkHealth {
            connected: s.connected,
            reconnects: s.reconnects,
            ..LinkHealth::default()
        }
    }
}

// ---------- Captura falsa ----------

#[derive(Default)]
struct ScreensState {
    senders: HashMap<DisplayId, watch::Sender<Option<Frame>>>,
    started: Vec<(DisplayId, (u32, u32), f32)>,
    fail: Option<CaptureError>,
}

/// Monitores falsos: la prueba publica los cuadros de cada uno con `publish`.
#[derive(Clone, Default)]
struct FakeScreens {
    state: Arc<Mutex<ScreensState>>,
    created: Arc<AtomicUsize>,
    stopped: Arc<AtomicUsize>,
}

impl FakeScreens {
    fn factory(&self) -> SourceFactory {
        let screens = self.clone();
        Arc::new(move || {
            screens.created.fetch_add(1, Ordering::SeqCst);
            Box::new(FakeSource(screens.clone()))
        })
    }

    fn publish(&self, display_id: &str, frame: Option<Frame>) {
        let mut s = self.state.lock().unwrap();
        let tx = s
            .senders
            .entry(display(display_id))
            .or_insert_with(|| watch::channel(None).0);
        tx.send_replace(frame);
    }

    fn created(&self) -> usize {
        self.created.load(Ordering::SeqCst)
    }

    fn stopped(&self) -> usize {
        self.stopped.load(Ordering::SeqCst)
    }
}

struct FakeSource(FakeScreens);

impl ScreenSource for FakeSource {
    fn displays(&self) -> std::result::Result<Vec<DisplayInfo>, CaptureError> {
        Ok(Vec::new())
    }

    fn start(
        &mut self,
        display: DisplayId,
        target: (u32, u32),
        fps: f32,
    ) -> std::result::Result<watch::Receiver<Option<Frame>>, CaptureError> {
        let mut s = self.0.state.lock().unwrap();
        if let Some(error) = s.fail.clone() {
            return Err(error);
        }
        s.started.push((display.clone(), target, fps));
        Ok(s.senders
            .entry(display)
            .or_insert_with(|| watch::channel(None).0)
            .subscribe())
    }

    fn stop(&mut self) {
        self.0.stopped.fetch_add(1, Ordering::SeqCst);
    }

    fn permission(&self) -> PermissionState {
        PermissionState::Granted
    }

    fn request_permission(&self) -> PermissionState {
        PermissionState::Granted
    }
}

// ---------- Utilidades ----------

fn base_settings() -> Settings {
    // Sin suavizado ni realce: así los colores de prueba salen tal cual
    Settings {
        smoothing_ms: 0.0,
        saturation: 1.0,
        ..Settings::default()
    }
}

fn spec(id: &str, preset: ZonePreset, light: &FakeLight) -> DeviceSpec {
    spec_with(id, preset, light, base_settings())
}

fn spec_with(id: &str, preset: ZonePreset, light: &FakeLight, settings: Settings) -> DeviceSpec {
    DeviceSpec {
        id: DeviceId(id.to_owned()),
        zones: vec![Zone::from_preset(display("1"), preset)],
        settings,
        calibration: Calibration::identity(),
        driver: light.driver(),
    }
}

async fn start(devices: Vec<DeviceSpec>, screens: &FakeScreens) -> Session {
    Session::start(
        devices,
        screens.factory(),
        &DeviceLocks::new(),
        SessionOptions::default(),
    )
    .await
    .unwrap()
}

fn hue_deg(c: &Hsv) -> f32 {
    c.h * 360.0
}

fn near_hue(c: &Hsv, expected: f32) -> bool {
    let d = (hue_deg(c) - expected).rem_euclid(360.0);
    d.min(360.0 - d) < 2.0
}

fn dev(id: &str) -> DeviceId {
    DeviceId(id.to_owned())
}

// ---------- Criterio de aceptación: 2 focos, zonas, límites, pausa y restauración ----------

#[tokio::test(start_paused = true)]
async fn dos_focos_siguen_cada_uno_su_zona_con_una_sola_captura() {
    let screens = FakeScreens::default();
    let (left, right) = (FakeLight::new(), FakeLight::new());
    screens.publish("1", Some(halves(ROJO, AZUL)));

    let session = start(
        vec![
            spec("izq", ZonePreset::Left, &left),
            spec("der", ZonePreset::Right, &right),
        ],
        &screens,
    )
    .await;
    sleep(Duration::from_millis(500)).await;
    session.stop().await;

    // Una captura por monitor usado, no por foco, al tamaño y fps pedidos
    assert_eq!(screens.created(), 1);
    assert_eq!(
        screens.state.lock().unwrap().started,
        vec![(display("1"), CAPTURE_SIZE, 20.0)]
    );
    assert_eq!(screens.stopped(), 1);

    assert!(
        near_hue(&left.sends()[0], 0.0),
        "izquierda roja: {:?}",
        left.sends()
    );
    assert!(
        near_hue(&right.sends()[0], 224.0),
        "derecha azul: {:?}",
        right.sends()
    );
    // Cada uno: preparar primero y restaurar al final
    for light in [&left, &right] {
        let log = light.log();
        assert_eq!(log.first(), Some(&Ev::Prepare));
        assert_eq!(log.last(), Some(&Ev::Restore));
        assert_eq!(light.count(Ev::Restore), 1);
    }
}

#[tokio::test(start_paused = true)]
async fn cada_foco_respeta_su_limite_de_envios() {
    let screens = FakeScreens::default();
    let (slow, fast) = (FakeLight::new(), FakeLight::new());
    let slow_settings = Settings {
        max_sends_per_s: 2.0,
        ..base_settings()
    };
    let session = start(
        vec![
            spec_with("lento", ZonePreset::Full, &slow, slow_settings),
            spec("rapido", ZonePreset::Full, &fast), // 8 por segundo
        ],
        &screens,
    )
    .await;
    // 4 segundos cambiando de color en cada cuadro
    for i in 0..80 {
        screens.publish("1", Some(solid(if i % 2 == 0 { ROJO } else { AZUL })));
        sleep(Duration::from_millis(50)).await;
    }
    session.stop().await;

    let (n_slow, n_fast) = (slow.sends().len(), fast.sends().len());
    assert!((7..=9).contains(&n_slow), "2 envíos/s en 4 s: {n_slow}");
    // Con cuadros cada 50 ms, 8 envíos/s (cada 125 ms) salen cada 150 ms: unos 27 en 4 s
    assert!((25..=33).contains(&n_fast), "~8 envíos/s en 4 s: {n_fast}");
    assert!(n_fast > n_slow * 3);
}

#[tokio::test(start_paused = true)]
async fn sin_pantalla_devuelve_los_focos_y_al_volver_retoma() {
    let screens = FakeScreens::default();
    let (a, b) = (FakeLight::new(), FakeLight::new());
    screens.publish("1", Some(solid(ROJO)));
    let session = start(
        vec![
            spec("a", ZonePreset::Left, &a),
            spec("b", ZonePreset::Right, &b),
        ],
        &screens,
    )
    .await;
    sleep(Duration::from_secs(1)).await;
    assert_eq!(session.state(), SessionState::Active);

    // Se va el monitor: antes de no_screen_wait_s (20 s) todavía no se toca nada
    screens.publish("1", None);
    sleep(Duration::from_secs(19)).await;
    assert_eq!(a.count(Ev::Restore), 0);
    sleep(Duration::from_secs(2)).await;
    for light in [&a, &b] {
        assert_eq!(light.count(Ev::Restore), 1, "se restaura una sola vez");
    }
    assert_eq!(session.state(), SessionState::Paused);
    assert_eq!(
        session.device_state(&dev("a")),
        Some(DeviceSessionState::Paused)
    );
    sleep(Duration::from_secs(30)).await;
    assert_eq!(
        a.count(Ev::Restore),
        1,
        "y no se repite mientras sigue sin pantalla"
    );

    // Vuelve el monitor: se prepara de nuevo y reenvía el color
    screens.publish("1", Some(solid(ROJO)));
    sleep(Duration::from_secs(1)).await;
    assert_eq!(session.state(), SessionState::Active);
    for light in [&a, &b] {
        assert_eq!(light.count(Ev::Prepare), 2);
        assert_eq!(light.sends().len(), 2, "color inicial y color al retomar");
    }
    session.stop().await;
    assert_eq!(a.count(Ev::Restore), 2);
}

#[tokio::test(start_paused = true)]
async fn detener_restaura_los_focos_y_los_libera() {
    let screens = FakeScreens::default();
    let (a, b) = (FakeLight::new(), FakeLight::new());
    let locks = DeviceLocks::new();
    screens.publish("1", Some(solid(VERDE)));
    let session = Session::start(
        vec![
            spec("a", ZonePreset::Left, &a),
            spec("b", ZonePreset::Right, &b),
        ],
        screens.factory(),
        &locks,
        SessionOptions::default(),
    )
    .await
    .unwrap();
    sleep(Duration::from_millis(300)).await;
    assert_eq!(locks.holder(&dev("a")), Some(Holder::Session));

    session.stop().await;
    for light in [&a, &b] {
        assert_eq!(light.log().last(), Some(&Ev::Restore));
    }
    assert_eq!(locks.holder(&dev("a")), None);
    assert_eq!(locks.holder(&dev("b")), None);
}

// ---------- Suspensión ----------

#[tokio::test(start_paused = true)]
async fn antes_de_dormir_restaura_y_al_despertar_retoma() {
    let screens = FakeScreens::default();
    let (a, b) = (FakeLight::new(), FakeLight::new());
    let power = PowerEvents::new();
    screens.publish("1", Some(solid(ROJO)));
    let session = Session::start(
        vec![
            spec("a", ZonePreset::Left, &a),
            spec("b", ZonePreset::Right, &b),
        ],
        screens.factory(),
        &DeviceLocks::new(),
        SessionOptions {
            power: Some(power.clone()),
            ..SessionOptions::default()
        },
    )
    .await
    .unwrap();
    sleep(Duration::from_secs(1)).await;

    power.emit(PowerEvent::Suspend);
    sleep(Duration::from_millis(200)).await;
    for light in [&a, &b] {
        assert_eq!(light.count(Ev::Restore), 1);
    }
    assert_eq!(session.state(), SessionState::Paused);
    // Dormido no se manda nada, aunque lleguen cuadros
    let sent = a.sends().len();
    screens.publish("1", Some(solid(AZUL)));
    sleep(Duration::from_secs(5)).await;
    assert_eq!(a.sends().len(), sent);

    power.emit(PowerEvent::Resume);
    sleep(Duration::from_secs(1)).await;
    assert_eq!(session.state(), SessionState::Active);
    for light in [&a, &b] {
        assert_eq!(light.count(Ev::Prepare), 2);
        assert!(
            near_hue(light.sends().last().unwrap(), 224.0),
            "retoma con el color actual"
        );
    }
    session.stop().await;
    // Ya estaban restaurados al detener? No: se retomó, así que se restaura otra vez
    assert_eq!(a.count(Ev::Restore), 2);
}

#[tokio::test(start_paused = true)]
async fn suspender_a_mano_hace_lo_mismo_que_el_evento() {
    let screens = FakeScreens::default();
    let light = FakeLight::new();
    screens.publish("1", Some(solid(ROJO)));
    let session = start(vec![spec("a", ZonePreset::Full, &light)], &screens).await;
    sleep(Duration::from_secs(1)).await;
    session.suspend().await;
    assert_eq!(light.count(Ev::Restore), 1);
    session.resume().await;
    sleep(Duration::from_secs(1)).await;
    assert_eq!(light.count(Ev::Prepare), 2);
    session.stop().await;
}

// ---------- Fallas ----------

#[tokio::test(start_paused = true)]
async fn un_foco_que_falla_no_frena_a_los_demas() {
    let screens = FakeScreens::default();
    let (broken, healthy) = (FakeLight::new(), FakeLight::new());
    broken.with(|s| s.fail_sends = true);
    let session = start(
        vec![
            spec("roto", ZonePreset::Full, &broken),
            spec("sano", ZonePreset::Full, &healthy),
        ],
        &screens,
    )
    .await;
    for i in 0..20 {
        screens.publish("1", Some(solid(if i % 2 == 0 { ROJO } else { AZUL })));
        sleep(Duration::from_millis(200)).await;
    }
    assert!(broken.sends().is_empty());
    assert!(healthy.sends().len() >= 15, "{}", healthy.sends().len());
    assert_eq!(
        session.device_state(&dev("sano")),
        Some(DeviceSessionState::Active)
    );
    session.stop().await;
    assert_eq!(broken.count(Ev::Restore), 1, "el roto también se restaura");
}

#[tokio::test(start_paused = true)]
async fn sin_conexion_el_color_espera_y_al_reconectar_se_reenvia() {
    let screens = FakeScreens::default();
    let light = FakeLight::new();
    screens.publish("1", Some(solid(ROJO)));
    light.with(|s| s.connected = false);
    let session = start(vec![spec("a", ZonePreset::Full, &light)], &screens).await;
    sleep(Duration::from_secs(2)).await;
    assert!(light.sends().is_empty());
    assert_eq!(
        session.device_state(&dev("a")),
        Some(DeviceSessionState::Reconnecting)
    );

    light.with(|s| s.connected = true);
    sleep(Duration::from_secs(1)).await;
    assert_eq!(light.sends().len(), 1);
    assert_eq!(
        session.device_state(&dev("a")),
        Some(DeviceSessionState::Active)
    );

    // Una reconexión del actor: el foco pudo perder el color
    light.with(|s| s.reconnects += 1);
    sleep(Duration::from_secs(1)).await;
    assert_eq!(light.sends().len(), 2);
    session.stop().await;
}

#[tokio::test(start_paused = true)]
async fn una_preparacion_fallida_se_reintenta_cada_retry_every_s() {
    let screens = FakeScreens::default();
    let light = FakeLight::new();
    screens.publish("1", Some(solid(ROJO)));
    light.with(|s| s.fail_prepare = Some(DeviceError::KeyRejected));
    let session = start(vec![spec("a", ZonePreset::Full, &light)], &screens).await;
    sleep(Duration::from_millis(500)).await;
    assert_eq!(
        session.device_state(&dev("a")),
        Some(DeviceSessionState::Error(
            openlight_types::ErrorCode::DeviceKeyRejected
        ))
    );
    let first = light.count(Ev::Prepare);
    // retry_every_s = 3 por defecto: en 10 s son ~3 reintentos, no uno por cuadro
    sleep(Duration::from_secs(10)).await;
    let attempts = light.count(Ev::Prepare) - first;
    assert!((2..=5).contains(&attempts), "{attempts}");

    // La clave se arregla
    light.with(|s| s.fail_prepare = None);
    sleep(Duration::from_secs(4)).await;
    assert_eq!(
        session.device_state(&dev("a")),
        Some(DeviceSessionState::Active)
    );
    assert!(!light.sends().is_empty());
    session.stop().await;
}

// ---------- Arranque ----------

#[tokio::test(start_paused = true)]
async fn un_foco_ocupado_impide_arrancar_sin_dejar_nada_a_medias() {
    let screens = FakeScreens::default();
    let (a, b) = (FakeLight::new(), FakeLight::new());
    let locks = DeviceLocks::new();
    let _calibrating = locks.try_acquire(&dev("b"), Holder::Calibration).unwrap();

    let error = Session::start(
        vec![
            spec("a", ZonePreset::Left, &a),
            spec("b", ZonePreset::Right, &b),
        ],
        screens.factory(),
        &locks,
        SessionOptions::default(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        error,
        CoreError::DeviceLocked {
            device: dev("b"),
            holder: "calibration".into()
        }
    );
    assert_eq!(
        locks.holder(&dev("a")),
        None,
        "el candado del primero se soltó"
    );
    assert_eq!(screens.created(), 0);
    assert!(a.log().is_empty() && b.log().is_empty());
}

#[tokio::test(start_paused = true)]
async fn dos_monitores_dos_capturas_y_un_foco_con_zonas_en_ambos() {
    let screens = FakeScreens::default();
    let (only1, only2, both) = (FakeLight::new(), FakeLight::new(), FakeLight::new());
    screens.publish("1", Some(solid(ROJO)));
    screens.publish("2", Some(solid(VERDE)));
    let both_spec = DeviceSpec {
        zones: vec![
            Zone::from_preset(display("1"), ZonePreset::Full),
            Zone::from_preset(display("2"), ZonePreset::Full),
        ],
        ..spec("ambos", ZonePreset::Full, &both)
    };
    let mut on_2 = spec("m2", ZonePreset::Full, &only2);
    on_2.zones = vec![Zone::from_preset(display("2"), ZonePreset::Full)];
    let session = start(
        vec![spec("m1", ZonePreset::Full, &only1), on_2, both_spec],
        &screens,
    )
    .await;
    sleep(Duration::from_millis(500)).await;
    session.stop().await;

    assert_eq!(screens.created(), 2, "una captura por monitor usado");
    assert!(near_hue(&only1.sends()[0], 0.0));
    assert!(near_hue(&only2.sends()[0], 131.0));
    let mixed = hue_deg(&both.sends()[0]);
    assert!(
        mixed > 40.0 && mixed < 100.0,
        "mezcla de rojo y verde: {mixed}"
    );
}

#[tokio::test(start_paused = true)]
async fn errores_de_arranque() {
    let screens = FakeScreens::default();
    let light = FakeLight::new();
    let locks = DeviceLocks::new();
    let run = |devices: Vec<DeviceSpec>, screens: &FakeScreens| {
        Session::start(
            devices,
            screens.factory(),
            &locks,
            SessionOptions::default(),
        )
    };

    // Sin focos con zonas
    let mut no_zones = spec("a", ZonePreset::Full, &light);
    no_zones.zones.clear();
    assert_eq!(
        run(vec![no_zones], &screens).await.err(),
        Some(CoreError::NothingToDo)
    );
    assert_eq!(
        run(vec![], &screens).await.err(),
        Some(CoreError::NothingToDo)
    );

    // Zona fuera del monitor
    let mut bad_zone = spec("a", ZonePreset::Full, &light);
    bad_zone.zones[0].rect = openlight_types::NormRect::new(0.8, 0.0, 0.5, 1.0);
    assert!(matches!(
        run(vec![bad_zone], &screens).await.err(),
        Some(CoreError::Invalid(_))
    ));

    // Falta el permiso de captura: se informa y se libera todo
    screens.state.lock().unwrap().fail = Some(CaptureError::PermissionDenied);
    let error = run(vec![spec("a", ZonePreset::Full, &light)], &screens)
        .await
        .err()
        .unwrap();
    assert_eq!(error, CoreError::Capture(CaptureError::PermissionDenied));
    assert_eq!(locks.holder(&dev("a")), None);
    let visible = openlight_types::UserFacingError::from(error);
    assert_eq!(
        visible.code,
        openlight_types::ErrorCode::ScreenPermissionDenied
    );
}

// ---------- Eventos ----------

#[tokio::test(start_paused = true)]
async fn publica_estado_color_en_vivo_y_estadisticas() {
    let screens = FakeScreens::default();
    let light = FakeLight::new();
    screens.publish("1", Some(solid(ROJO)));
    let session = start(vec![spec("a", ZonePreset::Full, &light)], &screens).await;
    let mut events = session.subscribe();
    // Cambia el color cada 100 ms durante un poco más de 2 s
    for i in 0..22 {
        screens.publish("1", Some(solid(if i % 2 == 0 { ROJO } else { AZUL })));
        sleep(Duration::from_millis(100)).await;
    }
    session.stop().await;

    let mut live = 0;
    let mut stats = Vec::new();
    let mut states = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            SessionEvent::Live(l) => {
                live += 1;
                assert_eq!(l.device, dev("a"));
            }
            SessionEvent::Stats(s) => stats.push(s),
            SessionEvent::State(s) => states.push(s.state),
            SessionEvent::Device(_) => {}
        }
    }
    assert!((15..=25).contains(&live), "≤10 Hz: {live}");
    assert!(!stats.is_empty());
    let sends: f32 = stats.iter().map(|s| s.devices[0].sends_per_s).sum();
    assert!(sends > 4.0, "envíos por segundo informados: {sends}");
    assert!(stats.iter().all(|s| s.capture_fps > 5.0));
    assert!(
        states.ends_with(&[SessionState::Restoring, SessionState::Idle]),
        "{states:?}"
    );
}
