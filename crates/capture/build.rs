//! Script de compilación de `capture`. Cada backend agrega solo sus líneas.

fn main() {
    // macOS (E1): `screencapturekit` trae un puente en Swift que necesita el runtime de Swift
    // del sistema (`/usr/lib/swift`). El rpath que agrega ese crate solo vale para sus propios
    // binarios, así que acá se repite para las pruebas y los ejemplos de `capture`. La app
    // (`app/src-tauri`) necesita lo mismo en su `build.rs`.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
