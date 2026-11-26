//! Auditoría del catálogo: **cada** ajuste, cambiado con la sesión andando, llega a donde tiene
//! efecto. El `match` es exhaustivo: un ajuste nuevo no compila hasta que se diga cómo se nota.
//!
//! Los que dependen de la conexión con el foco se siguen hasta el driver; de ahí al foco los
//! prueban `tuya-device` (`tests/en_marcha.rs`) y la app (`bridge/driver.rs`).

use openlight_types::{ColorMode, SendChannel};

use super::*;

const NEGRO: (u8, u8, u8) = (0, 0, 0);

/// Una sesión de un foco, a la que se le cambian los ajustes en marcha.
struct Bench {
    screens: FakeScreens,
    light: FakeLight,
    session: Session,
}

impl Bench {
    async fn new(first: Frame) -> Self {
        Self::with(first, FakeLight::new()).await
    }

    async fn with(first: Frame, light: FakeLight) -> Self {
        let screens = FakeScreens::default();
        screens.publish("1", Some(first));
        let session = start(vec![spec("a", ZonePreset::Full, &light)], &screens).await;
        let bench = Self {
            screens,
            light,
            session,
        };
        bench.wait(1.0).await;
        bench
    }

    async fn wait(&self, seconds: f64) {
        sleep(Duration::from_secs_f64(seconds)).await;
    }

    /// Los ajustes de base con uno cambiado.
    async fn set(&mut self, change: impl FnOnce(&mut Settings)) {
        let mut settings = base_settings();
        change(&mut settings);
        self.session
            .update_settings(vec![tuning("a", "1", ZonePreset::Full, settings)])
            .unwrap();
        self.wait(1.0).await;
    }

    async fn show(&self, frame: Frame) {
        self.screens.publish("1", Some(frame));
        self.wait(1.0).await;
    }

    fn last(&self) -> Hsv {
        *self.light.sends().last().expect("no se mandó nada")
    }

    fn applied(&self) -> Settings {
        self.light
            .0
            .lock()
            .unwrap()
            .applied
            .clone()
            .expect("al driver no le llegó nada")
    }
}

/// Un cuadro cuyo píxel `x` de cada fila (de 64) sale de `color`.
fn columns(color: impl Fn(u32) -> (u8, u8, u8)) -> Frame {
    let (w, h) = (64u32, 36u32);
    let mut bgra = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..h {
        for x in 0..w {
            let (r, g, b) = color(x);
            bgra.extend_from_slice(&[b, g, r, 255]);
        }
    }
    Frame::new(w, h, bgra).unwrap()
}

async fn check(id: SettingId) {
    match id {
        SettingId::ColorMode => {
            let mut b = Bench::new(columns(|x| if x < 40 { ROJO } else { AZUL })).await;
            assert!(near_hue(&b.last(), 0.0), "dominante: {:?}", b.last());
            b.set(|s| s.color_mode = ColorMode::Average).await;
            assert!(!near_hue(&b.last(), 0.0), "promedio: {:?}", b.last());
        }
        SettingId::Saturation => {
            let mut b = Bench::new(solid((200, 150, 150))).await;
            let before = b.last().s;
            b.set(|s| s.saturation = 2.0).await;
            assert!((b.last().s - 2.0 * before).abs() < 0.01, "{:?}", b.last());
        }
        SettingId::BrightnessMin => {
            let mut b = Bench::new(solid(NEGRO)).await;
            assert!((b.last().v - 0.05).abs() < 1e-3);
            b.set(|s| s.brightness_min = 0.4).await;
            assert!((b.last().v - 0.4).abs() < 1e-3, "{:?}", b.last());
        }
        SettingId::BrightnessMax => {
            let mut b = Bench::new(solid(BLANCO)).await;
            b.set(|s| s.brightness_max = 0.3).await;
            assert!((b.last().v - 0.3).abs() < 1e-3, "{:?}", b.last());
        }
        SettingId::SmoothingMs => {
            let mut b = Bench::new(solid(ROJO)).await;
            b.set(|s| s.smoothing_ms = 2000.0).await;
            b.screens.publish("1", Some(solid(AZUL)));
            b.wait(0.15).await;
            assert!(
                !near_hue(&b.last(), 224.0),
                "todavía en camino: {:?}",
                b.last()
            );
            b.wait(10.0).await;
            assert!(near_hue(&b.last(), 224.0), "{:?}", b.last());
        }
        SettingId::SendChannel => {
            let mut b = Bench::new(solid(ROJO)).await;
            let before = b.light.sends().len();
            b.set(|s| s.send_channel = SendChannel::ControlDirect).await;
            assert_eq!(b.applied().send_channel, SendChannel::ControlDirect);
            // Con la pantalla quieta, el color vuelve a salir (ya por el canal nuevo)
            assert_eq!(b.light.sends().len(), before + 1);
        }
        SettingId::CaptureFps => {
            let mut b = Bench::new(solid(ROJO)).await;
            b.set(|s| s.capture_fps = 40.0).await;
            let started = b.screens.state.lock().unwrap().started.clone();
            assert_eq!(started.last().unwrap().2, 40.0);
        }
        SettingId::MaxSendsPerS => {
            let mut b = Bench::new(solid(ROJO)).await;
            b.set(|s| s.max_sends_per_s = 1.0).await;
            let before = b.light.sends().len();
            for i in 0..20 {
                b.screens
                    .publish("1", Some(solid(if i % 2 == 0 { AZUL } else { VERDE })));
                b.wait(0.2).await;
            }
            let sent = b.light.sends().len() - before;
            assert!((1..=5).contains(&sent), "{sent} envíos en 4 s");
        }
        SettingId::SendThreshold => {
            let mut b = Bench::new(solid((200, 60, 60))).await;
            b.set(|s| s.send_threshold = 0.1).await;
            let before = b.light.sends().len();
            b.show(solid((200, 70, 60))).await;
            assert_eq!(b.light.sends().len(), before, "un cambio chico no sale");
        }
        SettingId::BlackBar => {
            // Bandas gris muy oscuro (0,15) a los costados de una imagen clara
            let framed = || {
                columns(|x| {
                    if (16..48).contains(&x) {
                        BLANCO
                    } else {
                        (38, 38, 38)
                    }
                })
            };
            let mut b = Bench::new(framed()).await;
            let with_bars = b.last().v;
            b.set(|s| s.black_bar = 0.2).await;
            assert!(
                b.last().v > with_bars + 0.05,
                "{with_bars} → {:?}",
                b.last()
            );
        }
        SettingId::GreyThreshold => {
            let mut b = Bench::new(solid((150, 135, 135))).await;
            assert!(b.last().s > 0.05, "con tono: {:?}", b.last());
            b.set(|s| s.grey_threshold = 0.2).await;
            assert_eq!(b.last().s, 0.0, "gris: {:?}", b.last());
        }
        SettingId::DarkScreen => {
            let mut b = Bench::new(solid(ROJO)).await;
            // Una escena gris con luz 0,2: con el defecto (0,12) se va a blanco
            b.show(solid((51, 51, 51))).await;
            assert_eq!(b.last().s, 0.0);
            b.show(solid(ROJO)).await;
            b.set(|s| s.dark_screen = 0.5).await;
            b.show(solid((51, 51, 51))).await;
            assert!(b.last().s > 0.5, "conserva el color: {:?}", b.last());
        }
        SettingId::HueStickiness => {
            let mostly_red = || columns(|x| if x < 36 { ROJO } else { AZUL });
            for (stickiness, expected) in [(1.0, 0.0), (3.0, 224.0)] {
                let mut b = Bench::new(solid(AZUL)).await;
                b.set(|s| s.hue_stickiness = stickiness).await;
                b.show(mostly_red()).await;
                assert!(
                    near_hue(&b.last(), expected),
                    "{stickiness}: {:?}",
                    b.last()
                );
            }
        }
        SettingId::NoScreenWaitS => {
            let mut b = Bench::new(solid(ROJO)).await;
            b.set(|s| s.no_screen_wait_s = 5.0).await;
            b.screens.publish("1", None);
            b.wait(7.0).await;
            assert_eq!(b.light.count(Ev::Restore), 1);
        }
        SettingId::HeartbeatS => {
            let mut b = Bench::new(solid(ROJO)).await;
            b.set(|s| s.heartbeat_s = 5.0).await;
            assert_eq!(b.applied().heartbeat_s, 5.0);
        }
        SettingId::DeadAfterS => {
            let mut b = Bench::new(solid(ROJO)).await;
            b.set(|s| s.dead_after_s = 60.0).await;
            assert_eq!(b.applied().dead_after_s, 60.0);
        }
        SettingId::RetryEveryS => {
            let light = FakeLight::new();
            light.with(|s| s.fail_prepare = Some(DeviceError::NotFound));
            let mut b = Bench::with(solid(ROJO), light).await;
            b.set(|s| s.retry_every_s = 1.0).await;
            // Pasa la espera que ya estaba en curso (3 s) y se cuentan los intentos
            b.wait(3.0).await;
            let before = b.light.count(Ev::Prepare);
            b.wait(5.0).await;
            let tries = b.light.count(Ev::Prepare) - before;
            assert!((4..=6).contains(&tries), "{tries} intentos en 5 s");
            assert_eq!(b.applied().retry_every_s, 1.0);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn cada_ajuste_del_catalogo_tiene_efecto_en_marcha() {
    for id in SettingId::ALL {
        println!("ajuste: {id}");
        check(id).await;
    }
}
