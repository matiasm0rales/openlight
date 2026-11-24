fn main() {
    let mut attributes = tauri_build::Attributes::new();
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // tauri-build solo agrega el manifiesto de Windows al ejecutable de la app; los binarios
        // de `cargo test` quedan sin Common Controls v6 y no arrancan (STATUS_ENTRYPOINT_NOT_FOUND).
        // Se lo agregamos a todos los binarios con el linker.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        let manifest =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        println!("cargo:rustc-link-arg=/WX");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // `capture` trae un puente en Swift (ScreenCaptureKit) que necesita el runtime de Swift del
        // sistema; sin este rpath ni la app ni sus pruebas arrancan (dyld: libswift_Concurrency).
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    tauri_build::try_build(attributes).expect("falló tauri-build");
}
