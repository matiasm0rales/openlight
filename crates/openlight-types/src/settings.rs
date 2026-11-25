//! Catálogo de ajustes: **la única fuente de verdad** de ids, tipos, valores por defecto, rangos,
//! pasos, unidades, niveles y claves i18n. La UI genera los controles desde acá y el core valida
//! contra acá. Los valores por defecto son los del script `ambilight.py`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{ColorMode, ErrorCode, SendChannel, UserFacingError};

/// Identificador de un ajuste. El nombre serde (`snake_case`) es el id estable.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum SettingId {
    /// Dominante o promedio.
    ColorMode,
    /// Realce de colores.
    Saturation,
    /// Brillo con pantalla oscura.
    BrightnessMin,
    /// Tope de brillo.
    BrightnessMax,
    /// Constante de tiempo del suavizado, en ms.
    SmoothingMs,
    /// Canal del foco.
    SendChannel,
    /// Capturas por segundo.
    CaptureFps,
    /// Comandos por segundo como máximo.
    MaxSendsPerS,
    /// Cambios más chicos que esto no se mandan.
    SendThreshold,
    /// Brillo por debajo del cual una fila o columna es banda negra.
    BlackBar,
    /// Saturación por debajo de la cual la pantalla es gris.
    GreyThreshold,
    /// Por debajo de este brillo se conserva el último color.
    DarkScreen,
    /// Ventaja del tono previo.
    HueStickiness,
    /// Segundos sin pantalla antes de devolver el foco a su estado.
    NoScreenWaitS,
    /// Segundos entre señales de vida.
    HeartbeatS,
    /// Segundos sin respuesta para dar la conexión por muerta.
    DeadAfterS,
    /// Segundos entre intentos de reconexión.
    RetryEveryS,
}

impl SettingId {
    /// Todos los ajustes, en el orden en que se muestran.
    pub const ALL: [Self; 17] = [
        Self::ColorMode,
        Self::Saturation,
        Self::BrightnessMin,
        Self::BrightnessMax,
        Self::SmoothingMs,
        Self::SendChannel,
        Self::CaptureFps,
        Self::MaxSendsPerS,
        Self::SendThreshold,
        Self::BlackBar,
        Self::GreyThreshold,
        Self::DarkScreen,
        Self::HueStickiness,
        Self::NoScreenWaitS,
        Self::HeartbeatS,
        Self::DeadAfterS,
        Self::RetryEveryS,
    ];

    /// Id estable (`max_sends_per_s`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ColorMode => "color_mode",
            Self::Saturation => "saturation",
            Self::BrightnessMin => "brightness_min",
            Self::BrightnessMax => "brightness_max",
            Self::SmoothingMs => "smoothing_ms",
            Self::SendChannel => "send_channel",
            Self::CaptureFps => "capture_fps",
            Self::MaxSendsPerS => "max_sends_per_s",
            Self::SendThreshold => "send_threshold",
            Self::BlackBar => "black_bar",
            Self::GreyThreshold => "grey_threshold",
            Self::DarkScreen => "dark_screen",
            Self::HueStickiness => "hue_stickiness",
            Self::NoScreenWaitS => "no_screen_wait_s",
            Self::HeartbeatS => "heartbeat_s",
            Self::DeadAfterS => "dead_after_s",
            Self::RetryEveryS => "retry_every_s",
        }
    }

    /// Cómo llega un cambio de este ajuste a una sesión que ya está andando.
    pub const fn apply(self) -> SettingApply {
        match self {
            Self::CaptureFps => SettingApply::RestartCapture,
            Self::ColorMode
            | Self::Saturation
            | Self::BrightnessMin
            | Self::BrightnessMax
            | Self::SmoothingMs
            | Self::SendChannel
            | Self::MaxSendsPerS
            | Self::SendThreshold
            | Self::BlackBar
            | Self::GreyThreshold
            | Self::DarkScreen
            | Self::HueStickiness
            | Self::NoScreenWaitS
            | Self::HeartbeatS
            | Self::DeadAfterS
            | Self::RetryEveryS => SettingApply::Live,
        }
    }

    /// Definición del ajuste en el catálogo.
    pub fn def(self) -> &'static SettingDef {
        catalog()
            .iter()
            .find(|d| d.id == self)
            .expect("todo SettingId está en el catálogo (lo verifica una prueba)")
    }
}

impl fmt::Display for SettingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Cómo llega un cambio de ajuste a una sesión que ya está andando.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingApply {
    /// Al instante, sin cortar nada.
    Live,
    /// Hay que rearmar la captura de pantalla. Las conexiones con los focos siguen.
    RestartCapture,
}

/// Nivel de un ajuste en la pantalla de Ajustes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum SettingLevel {
    /// Lo que cualquiera quiere tocar.
    Basic,
    /// Para afinar.
    Advanced,
    /// Conexión con el foco; solo si algo anda mal.
    Expert,
}

/// Unidad de un ajuste numérico (la UI la muestra junto al valor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SettingUnit {
    /// Sin unidad: fracción `0..1` (la UI la puede mostrar como %).
    Fraction,
    /// Multiplicador (`×1.3`).
    Factor,
    /// Milisegundos.
    Milliseconds,
    /// Segundos.
    Seconds,
    /// Cuadros por segundo.
    Fps,
    /// Veces por segundo.
    PerSecond,
}

/// Tipo y restricciones de un ajuste.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SettingKind {
    /// Número en `[min, max]` con paso `step`.
    Number {
        /// Mínimo (incluido).
        min: f64,
        /// Máximo (incluido).
        max: f64,
        /// Paso del control.
        step: f64,
        /// Unidad.
        unit: SettingUnit,
    },
    /// Una opción de una lista. Cada opción tiene su clave i18n
    /// `settings.<id>.option.<opción>`.
    Choice {
        /// Opciones válidas (ids estables).
        options: Vec<String>,
    },
}

/// Valor de un ajuste: número u opción. En TypeScript es `number | string`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(untagged)]
pub enum SettingValue {
    /// Valor numérico.
    Number(f64),
    /// Opción.
    Choice(String),
}

impl SettingValue {
    /// El número, si lo es.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Choice(_) => None,
        }
    }

    /// La opción, si lo es.
    pub fn as_choice(&self) -> Option<&str> {
        match self {
            Self::Choice(c) => Some(c),
            Self::Number(_) => None,
        }
    }
}

/// Definición de un ajuste.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SettingDef {
    /// Id.
    pub id: SettingId,
    /// Tipo y restricciones.
    pub kind: SettingKind,
    /// Valor por defecto (el del script).
    pub default: SettingValue,
    /// Nivel.
    pub level: SettingLevel,
    /// Clave i18n del título: `settings.<id>.title`.
    pub title_key: String,
    /// Clave i18n de la explicación: `settings.<id>.help`.
    pub help_key: String,
}

impl SettingDef {
    fn new(id: SettingId, kind: SettingKind, default: SettingValue, level: SettingLevel) -> Self {
        Self {
            id,
            kind,
            default,
            level,
            title_key: format!("settings.{id}.title"),
            help_key: format!("settings.{id}.help"),
        }
    }

    /// Verifica que `value` sea válido para este ajuste.
    pub fn validate(&self, value: &SettingValue) -> Result<(), SettingsError> {
        match (&self.kind, value) {
            (SettingKind::Number { min, max, .. }, SettingValue::Number(v)) => {
                const EPS: f64 = 1e-9;
                if v.is_finite() && *v >= min - EPS && *v <= max + EPS {
                    Ok(())
                } else {
                    Err(SettingsError::OutOfRange {
                        id: self.id,
                        value: *v,
                        min: *min,
                        max: *max,
                    })
                }
            }
            (SettingKind::Choice { options }, SettingValue::Choice(c)) => {
                if options.iter().any(|o| o == c) {
                    Ok(())
                } else {
                    Err(SettingsError::UnknownChoice {
                        id: self.id,
                        value: c.clone(),
                    })
                }
            }
            _ => Err(SettingsError::WrongType { id: self.id }),
        }
    }
}

fn number(min: f64, max: f64, step: f64, unit: SettingUnit) -> SettingKind {
    SettingKind::Number {
        min,
        max,
        step,
        unit,
    }
}

fn choice(options: &[&str]) -> SettingKind {
    SettingKind::Choice {
        options: options.iter().map(|o| (*o).to_owned()).collect(),
    }
}

static CATALOG: LazyLock<Vec<SettingDef>> = LazyLock::new(|| {
    use SettingId as I;
    use SettingLevel::{Advanced, Basic, Expert};
    use SettingUnit as U;
    let n = SettingValue::Number;
    let c = |s: &str| SettingValue::Choice(s.to_owned());
    vec![
        SettingDef::new(
            I::ColorMode,
            choice(&["dominant", "average"]),
            c("dominant"),
            Basic,
        ),
        SettingDef::new(
            I::Saturation,
            number(0.5, 2.5, 0.05, U::Factor),
            n(1.3),
            Basic,
        ),
        SettingDef::new(
            I::BrightnessMin,
            number(0.0, 1.0, 0.01, U::Fraction),
            n(0.05),
            Basic,
        ),
        SettingDef::new(
            I::BrightnessMax,
            number(0.0, 1.0, 0.01, U::Fraction),
            n(1.0),
            Basic,
        ),
        SettingDef::new(
            I::SmoothingMs,
            number(0.0, 2000.0, 10.0, U::Milliseconds),
            n(300.0),
            Basic,
        ),
        SettingDef::new(
            I::SendChannel,
            choice(&["colour", "control_direct", "control_gradual"]),
            c("colour"),
            Advanced,
        ),
        SettingDef::new(
            I::CaptureFps,
            number(5.0, 60.0, 1.0, U::Fps),
            n(20.0),
            Advanced,
        ),
        SettingDef::new(
            I::MaxSendsPerS,
            number(1.0, 20.0, 1.0, U::PerSecond),
            n(8.0),
            Advanced,
        ),
        SettingDef::new(
            I::SendThreshold,
            number(0.0, 0.1, 0.005, U::Fraction),
            n(0.01),
            Advanced,
        ),
        SettingDef::new(
            I::BlackBar,
            number(0.0, 0.3, 0.01, U::Fraction),
            n(0.06),
            Advanced,
        ),
        SettingDef::new(
            I::GreyThreshold,
            number(0.0, 0.2, 0.005, U::Fraction),
            n(0.02),
            Advanced,
        ),
        SettingDef::new(
            I::DarkScreen,
            number(0.0, 0.5, 0.01, U::Fraction),
            n(0.12),
            Advanced,
        ),
        SettingDef::new(
            I::HueStickiness,
            number(1.0, 3.0, 0.05, U::Factor),
            n(1.4),
            Advanced,
        ),
        SettingDef::new(
            I::NoScreenWaitS,
            number(5.0, 300.0, 1.0, U::Seconds),
            n(20.0),
            Advanced,
        ),
        SettingDef::new(
            I::HeartbeatS,
            number(3.0, 30.0, 1.0, U::Seconds),
            n(10.0),
            Expert,
        ),
        SettingDef::new(
            I::DeadAfterS,
            number(5.0, 120.0, 1.0, U::Seconds),
            n(30.0),
            Expert,
        ),
        SettingDef::new(
            I::RetryEveryS,
            number(1.0, 30.0, 1.0, U::Seconds),
            n(3.0),
            Expert,
        ),
    ]
});

/// El catálogo completo, en el orden de [`SettingId::ALL`].
pub fn catalog() -> &'static [SettingDef] {
    &CATALOG
}

/// Errores al validar ajustes.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SettingsError {
    /// Número fuera de rango.
    #[error("{id} = {value} está fuera de [{min}, {max}]")]
    OutOfRange {
        /// Ajuste.
        id: SettingId,
        /// Valor recibido.
        value: f64,
        /// Mínimo.
        min: f64,
        /// Máximo.
        max: f64,
    },
    /// Opción que no existe.
    #[error("{id} no acepta la opción {value:?}")]
    UnknownChoice {
        /// Ajuste.
        id: SettingId,
        /// Valor recibido.
        value: String,
    },
    /// Número donde va una opción o al revés.
    #[error("{id} recibió un valor del tipo equivocado")]
    WrongType {
        /// Ajuste.
        id: SettingId,
    },
}

impl From<SettingsError> for UserFacingError {
    fn from(e: SettingsError) -> Self {
        let id = match &e {
            SettingsError::OutOfRange { id, .. }
            | SettingsError::UnknownChoice { id, .. }
            | SettingsError::WrongType { id } => *id,
        };
        UserFacingError::new(ErrorCode::InvalidSetting).with_param("setting", id.as_str())
    }
}

/// Valores que pisa un foco o un perfil sobre los globales (solo los que cambian).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct SettingsOverride(pub BTreeMap<SettingId, SettingValue>);

impl SettingsOverride {
    /// Verifica todos los valores contra el catálogo.
    pub fn validate(&self) -> Result<(), SettingsError> {
        self.0.iter().try_for_each(|(id, v)| id.def().validate(v))
    }
}

/// Valores efectivos de todos los ajustes, tipados. `Default` sale del catálogo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Settings {
    /// `color_mode`.
    pub color_mode: ColorMode,
    /// `saturation`.
    pub saturation: f32,
    /// `brightness_min`.
    pub brightness_min: f32,
    /// `brightness_max`.
    pub brightness_max: f32,
    /// `smoothing_ms`.
    pub smoothing_ms: f32,
    /// `send_channel`.
    pub send_channel: SendChannel,
    /// `capture_fps`.
    pub capture_fps: f32,
    /// `max_sends_per_s`.
    pub max_sends_per_s: f32,
    /// `send_threshold`.
    pub send_threshold: f32,
    /// `black_bar`.
    pub black_bar: f32,
    /// `grey_threshold`.
    pub grey_threshold: f32,
    /// `dark_screen`.
    pub dark_screen: f32,
    /// `hue_stickiness`.
    pub hue_stickiness: f32,
    /// `no_screen_wait_s`.
    pub no_screen_wait_s: f32,
    /// `heartbeat_s`.
    pub heartbeat_s: f32,
    /// `dead_after_s`.
    pub dead_after_s: f32,
    /// `retry_every_s`.
    pub retry_every_s: f32,
}

impl Default for Settings {
    fn default() -> Self {
        let mut s = Self {
            color_mode: ColorMode::Dominant,
            saturation: 0.0,
            brightness_min: 0.0,
            brightness_max: 0.0,
            smoothing_ms: 0.0,
            send_channel: SendChannel::Colour,
            capture_fps: 0.0,
            max_sends_per_s: 0.0,
            send_threshold: 0.0,
            black_bar: 0.0,
            grey_threshold: 0.0,
            dark_screen: 0.0,
            hue_stickiness: 0.0,
            no_screen_wait_s: 0.0,
            heartbeat_s: 0.0,
            dead_after_s: 0.0,
            retry_every_s: 0.0,
        };
        for def in catalog() {
            s.set(def.id, def.default.clone())
                .expect("los defaults son válidos (prueba)");
        }
        s
    }
}

impl Settings {
    /// Valor actual de un ajuste.
    pub fn get(&self, id: SettingId) -> SettingValue {
        // f32 → f64 por su representación decimal más corta: 1.3f32 vuelve como 1.3, no 1.2999999523.
        let n = |v: f32| SettingValue::Number(v.to_string().parse().unwrap_or(f64::from(v)));
        let c = |v: &str| SettingValue::Choice(v.to_owned());
        match id {
            SettingId::ColorMode => c(self.color_mode.as_str()),
            SettingId::SendChannel => c(self.send_channel.as_str()),
            SettingId::Saturation => n(self.saturation),
            SettingId::BrightnessMin => n(self.brightness_min),
            SettingId::BrightnessMax => n(self.brightness_max),
            SettingId::SmoothingMs => n(self.smoothing_ms),
            SettingId::CaptureFps => n(self.capture_fps),
            SettingId::MaxSendsPerS => n(self.max_sends_per_s),
            SettingId::SendThreshold => n(self.send_threshold),
            SettingId::BlackBar => n(self.black_bar),
            SettingId::GreyThreshold => n(self.grey_threshold),
            SettingId::DarkScreen => n(self.dark_screen),
            SettingId::HueStickiness => n(self.hue_stickiness),
            SettingId::NoScreenWaitS => n(self.no_screen_wait_s),
            SettingId::HeartbeatS => n(self.heartbeat_s),
            SettingId::DeadAfterS => n(self.dead_after_s),
            SettingId::RetryEveryS => n(self.retry_every_s),
        }
    }

    /// Cambia un ajuste después de validarlo contra el catálogo.
    pub fn set(&mut self, id: SettingId, value: SettingValue) -> Result<(), SettingsError> {
        id.def().validate(&value)?;
        if let SettingValue::Choice(c) = &value {
            match id {
                SettingId::ColorMode => {
                    self.color_mode = ColorMode::parse(c).ok_or(SettingsError::WrongType { id })?;
                }
                SettingId::SendChannel => {
                    self.send_channel =
                        SendChannel::parse(c).ok_or(SettingsError::WrongType { id })?;
                }
                _ => return Err(SettingsError::WrongType { id }),
            }
            return Ok(());
        }
        let v = value.as_number().ok_or(SettingsError::WrongType { id })? as f32;
        let field = match id {
            SettingId::Saturation => &mut self.saturation,
            SettingId::BrightnessMin => &mut self.brightness_min,
            SettingId::BrightnessMax => &mut self.brightness_max,
            SettingId::SmoothingMs => &mut self.smoothing_ms,
            SettingId::CaptureFps => &mut self.capture_fps,
            SettingId::MaxSendsPerS => &mut self.max_sends_per_s,
            SettingId::SendThreshold => &mut self.send_threshold,
            SettingId::BlackBar => &mut self.black_bar,
            SettingId::GreyThreshold => &mut self.grey_threshold,
            SettingId::DarkScreen => &mut self.dark_screen,
            SettingId::HueStickiness => &mut self.hue_stickiness,
            SettingId::NoScreenWaitS => &mut self.no_screen_wait_s,
            SettingId::HeartbeatS => &mut self.heartbeat_s,
            SettingId::DeadAfterS => &mut self.dead_after_s,
            SettingId::RetryEveryS => &mut self.retry_every_s,
            SettingId::ColorMode | SettingId::SendChannel => {
                return Err(SettingsError::WrongType { id });
            }
        };
        *field = v;
        Ok(())
    }

    /// Copia con los valores de `overrides` aplicados (globales → perfil → foco).
    pub fn with_overrides(&self, overrides: &SettingsOverride) -> Result<Self, SettingsError> {
        let mut s = self.clone();
        for (id, v) in &overrides.0 {
            s.set(*id, v.clone())?;
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_catalogo_tiene_cada_ajuste_una_vez_y_en_orden() {
        let ids: Vec<_> = catalog().iter().map(|d| d.id).collect();
        assert_eq!(ids, SettingId::ALL.to_vec());
    }

    #[test]
    fn los_defaults_estan_dentro_de_sus_rangos() {
        for def in catalog() {
            def.validate(&def.default).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn los_defaults_caen_en_la_grilla_del_paso() {
        for def in catalog() {
            if let (SettingKind::Number { min, max, step, .. }, SettingValue::Number(v)) =
                (&def.kind, &def.default)
            {
                assert!(min < max && *step > 0.0, "{}", def.id);
                let k = (v - min) / step;
                assert!(
                    (k - k.round()).abs() < 1e-6,
                    "{} = {v} no cae en el paso {step}",
                    def.id
                );
            }
        }
    }

    #[test]
    fn los_nombres_serde_son_los_ids_estables() {
        for id in SettingId::ALL {
            assert_eq!(serde_json::to_value(id).unwrap(), id.as_str());
        }
    }

    // port: ambilight.py, constantes y valores por defecto de argparse
    #[test]
    fn los_defaults_son_los_del_script() {
        let s = Settings::default();
        assert_eq!(s.color_mode, ColorMode::Dominant);
        assert_eq!(s.send_channel, SendChannel::Colour);
        let esperados = [
            (s.saturation, 1.3),
            (s.brightness_min, 0.05),
            (s.brightness_max, 1.0),
            (s.smoothing_ms, 300.0),
            (s.capture_fps, 20.0),
            (s.max_sends_per_s, 8.0),
            (s.send_threshold, 0.01),   // UMBRAL_ENVIO
            (s.black_bar, 0.06),        // BANDA_NEGRA
            (s.grey_threshold, 0.02),   // SIN_COLOR
            (s.dark_screen, 0.12),      // PANTALLA_OSCURA
            (s.hue_stickiness, 1.4),    // VENTAJA_TONO_PREVIO
            (s.no_screen_wait_s, 20.0), // ESPERA_SIN_PANTALLA
            (s.heartbeat_s, 10.0),      // Foco.LATIDO_CADA
            (s.dead_after_s, 30.0),     // Foco.SIN_RESPUESTA_MAX
            (s.retry_every_s, 3.0),     // Foco.REINTENTO_CADA
        ];
        for (real, esperado) in esperados {
            assert_eq!(real, esperado);
        }
        assert!(s.brightness_min <= s.brightness_max);
    }

    #[test]
    fn get_devuelve_lo_que_se_puso() {
        let mut s = Settings::default();
        for def in catalog() {
            assert_eq!(s.get(def.id), def.default, "{}", def.id);
        }
        s.set(SettingId::Saturation, SettingValue::Number(2.0))
            .unwrap();
        assert_eq!(s.get(SettingId::Saturation), SettingValue::Number(2.0));
        s.set(
            SettingId::SendChannel,
            SettingValue::Choice("control_gradual".into()),
        )
        .unwrap();
        assert_eq!(s.send_channel, SendChannel::ControlGradual);
    }

    #[test]
    fn rechaza_valores_invalidos() {
        let mut s = Settings::default();
        assert!(matches!(
            s.set(SettingId::Saturation, SettingValue::Number(9.0)),
            Err(SettingsError::OutOfRange { .. })
        ));
        assert!(
            s.set(SettingId::Saturation, SettingValue::Number(f64::NAN))
                .is_err()
        );
        assert!(matches!(
            s.set(SettingId::ColorMode, SettingValue::Choice("rainbow".into())),
            Err(SettingsError::UnknownChoice { .. })
        ));
        assert!(matches!(
            s.set(SettingId::ColorMode, SettingValue::Number(1.0)),
            Err(SettingsError::WrongType { .. })
        ));
        assert_eq!(s, Settings::default());
    }

    #[test]
    fn overrides_por_foco() {
        let mut ov = SettingsOverride::default();
        ov.0.insert(SettingId::BrightnessMax, SettingValue::Number(0.5));
        let s = Settings::default().with_overrides(&ov).unwrap();
        assert_eq!(s.brightness_max, 0.5);
        let json = serde_json::to_string(&ov).unwrap();
        assert_eq!(json, r#"{"brightness_max":0.5}"#);
    }
}
