//! Captura de pantalla multiplataforma: implementaciones de ScreenSource para macOS, Windows y Linux.
//!
//! Esqueleto creado en wave 0. Lo implementa la tarea **E1/E2/E3 (wave 1)** de `docs/PLAN.md`;
//! esa tarea es dueña de esta carpeta (ver `docs/AGENTS.md`).
//!
//! Cada backend vive en su carpeta y tiene su propio dueño: `src/macos` (E1),
//! `src/windows` (E2) y `src/linux` (E3).

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
