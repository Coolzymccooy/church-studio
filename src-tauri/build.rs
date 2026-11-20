use std::path::Path;

/// The NDI® runtime that ships inside the app (see `tauri.macos.conf.json` and
/// `tauri.windows.conf.json`). It is fetched from NDI's SDK at build time, never
/// committed, so say how to get it before tauri-build fails on a missing file.
fn bundled_ndi_runtime(target_os: &str) -> Option<&'static str> {
    match target_os {
        "macos" => Some("ndi-runtime/macos/libndi.dylib"),
        "windows" => Some("ndi-runtime/windows-x64/Processing.NDI.Lib.x64.dll"),
        _ => None,
    }
}

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if let Some(lib) = bundled_ndi_runtime(&target_os) {
        println!("cargo:rerun-if-changed={lib}");
        if !Path::new(lib).exists() {
            panic!(
                "\n\nThe bundled NDI runtime is missing: src-tauri/{lib}\n\
                 Run `npm run ndi:fetch` once (it downloads NDI's official SDK and keeps a cached copy).\n\n"
            );
        }
    }
    tauri_build::build()
}
