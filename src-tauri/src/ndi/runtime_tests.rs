//! Unit tests for the runtime search order (`runtime::candidate_paths`).
//! They need no NDI runtime.
use super::runtime::{
    bundled_paths, candidate_paths, library_file_name, status_from, TargetOs,
    MACOS_NDI_TOOLS_LIBS, NOT_INSTALLED,
};
use std::path::{Path, PathBuf};
use std::collections::HashMap;

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |key: &str| map.get(key).cloned()
}

#[test]
fn without_an_app_folder_env_dirs_come_first_v6_then_v5() {
    let paths = candidate_paths(
        env_of(&[("NDI_RUNTIME_DIR_V5", "/opt/v5"), ("NDI_RUNTIME_DIR_V6", "/opt/v6")]),
        TargetOs::Windows,
        None,
    );
    let lib = library_file_name(TargetOs::Windows);
    assert_eq!(paths[0], Path::new("/opt/v6").join(lib));
    assert_eq!(paths[1], Path::new("/opt/v5").join(lib));
    assert_eq!(
        paths[2],
        PathBuf::from(format!(r"C:\Program Files\NDI\NDI 6 Runtime\v6\{lib}"))
    );
    assert_eq!(
        paths[3],
        PathBuf::from(format!(r"C:\Program Files\NDI\NDI 5 Runtime\v5\{lib}"))
    );
    assert_eq!(paths.len(), 4);
}

#[test]
fn windows_x64_uses_the_x64_dll() {
    if cfg!(target_pointer_width = "64") {
        assert_eq!(library_file_name(TargetOs::Windows), "Processing.NDI.Lib.x64.dll");
    }
}

#[test]
fn blank_env_values_are_ignored() {
    let paths = candidate_paths(env_of(&[("NDI_RUNTIME_DIR_V6", "  ")]), TargetOs::MacOs, None);
    assert_eq!(&paths[..2], &[
        PathBuf::from("/Library/NDI SDK for Apple/lib/macOS/libndi.dylib"),
        PathBuf::from("/usr/local/lib/libndi.dylib"),
    ]);
    assert_eq!(paths.len(), 2 + MACOS_NDI_TOOLS_LIBS.len());
}

#[test]
fn macos_falls_back_to_the_copies_ndi_tools_installs_after_the_app_folder() {
    let paths = candidate_paths(env_of(&[]), TargetOs::MacOs, Some(Path::new("/App/Contents/MacOS")));
    let app = paths.iter().position(|p| p == Path::new("/App/Contents/MacOS/libndi.dylib")).unwrap();
    let tools: Vec<usize> = MACOS_NDI_TOOLS_LIBS
        .iter()
        .map(|t| paths.iter().position(|p| p == Path::new(t)).unwrap())
        .collect();
    assert!(tools.iter().all(|&i| i > app), "NDI Tools copies come after the app's own folder");
    assert!(tools.windows(2).all(|w| w[0] < w[1]), "in the listed order");
    // the standard library is tried before the Advanced build
    let first_advanced = MACOS_NDI_TOOLS_LIBS.iter().position(|p| p.ends_with("libndi_advanced.dylib")).unwrap();
    assert!(MACOS_NDI_TOOLS_LIBS[..first_advanced].iter().all(|p| p.ends_with("/libndi.dylib")));
}

#[test]
fn other_systems_never_look_in_ndi_tools_mac_apps() {
    for os in [TargetOs::Windows, TargetOs::Linux, TargetOs::Other] {
        let paths = candidate_paths(env_of(&[]), os, None);
        assert!(MACOS_NDI_TOOLS_LIBS.iter().all(|t| !paths.contains(&PathBuf::from(t))));
    }
}

#[test]
fn linux_order_and_versioned_env_names() {
    let paths = candidate_paths(
        env_of(&[("NDI_RUNTIME_DIR_V5", "/opt/ndi")]),
        TargetOs::Linux,
        Some(Path::new("/app")),
    );
    assert_eq!(
        paths,
        vec![
            Path::new("/app").join("libndi.so.6"),
            Path::new("/opt/ndi").join("libndi.so.5"),
            PathBuf::from("libndi.so.6"),
            PathBuf::from("libndi.so.5"),
            PathBuf::from("/usr/lib/libndi.so"),
        ]
    );
}

#[test]
fn duplicates_are_removed() {
    let paths = candidate_paths(
        env_of(&[("NDI_RUNTIME_DIR_V6", "/same"), ("NDI_RUNTIME_DIR_V5", "/same")]),
        TargetOs::Windows,
        Some(Path::new("/same")),
    );
    let lib = library_file_name(TargetOs::Windows);
    assert_eq!(paths.first(), Some(&Path::new("/same").join(lib)));
    // /same once, then the two Program Files locations.
    assert_eq!(paths.len(), 3);
}

// ── The copy bundled with the app comes first ────────────────────────────────

#[test]
fn macos_app_bundle_is_searched_before_anything_installed() {
    let paths = candidate_paths(
        env_of(&[("NDI_RUNTIME_DIR_V6", "/opt/v6")]),
        TargetOs::MacOs,
        Some(Path::new("/Applications/TIWATON AI Studio.app/Contents/MacOS")),
    );
    let contents = Path::new("/Applications/TIWATON AI Studio.app/Contents");
    assert_eq!(
        &paths[..6],
        &[
            contents.join("Frameworks/libndi.dylib"),
            contents.join("Resources/libndi.dylib"),
            contents.join("MacOS/libndi.dylib"),
            Path::new("/opt/v6").join("libndi.dylib"),
            PathBuf::from("/Library/NDI SDK for Apple/lib/macOS/libndi.dylib"),
            PathBuf::from("/usr/local/lib/libndi.dylib"),
        ]
    );
    assert_eq!(&paths[6..], MACOS_NDI_TOOLS_LIBS.map(PathBuf::from).as_slice());
}

#[test]
fn macos_dev_build_finds_the_copy_tauri_build_puts_in_target_frameworks() {
    let paths = candidate_paths(env_of(&[]), TargetOs::MacOs, Some(Path::new("/repo/src-tauri/target/debug")));
    assert_eq!(paths[0], PathBuf::from("/repo/src-tauri/target/Frameworks/libndi.dylib"));
}

#[test]
fn windows_dll_next_to_the_exe_beats_the_installed_runtimes() {
    let lib = library_file_name(TargetOs::Windows);
    let paths = candidate_paths(
        env_of(&[("NDI_RUNTIME_DIR_V6", r"C:\NDI")]),
        TargetOs::Windows,
        Some(Path::new(r"C:\Users\me\AppData\Local\TIWATON AI Studio")),
    );
    assert_eq!(paths[0], Path::new(r"C:\Users\me\AppData\Local\TIWATON AI Studio").join(lib));
    assert_eq!(paths[1], Path::new(r"C:\NDI").join(lib));
    assert_eq!(
        paths[2],
        PathBuf::from(format!(r"C:\Program Files\NDI\NDI 6 Runtime\v6\{lib}"))
    );
    assert_eq!(paths.len(), 4);
}

#[test]
fn bundled_paths_per_platform() {
    let exe = Path::new("/x/Contents/MacOS");
    assert_eq!(
        bundled_paths(TargetOs::MacOs, exe),
        vec![
            PathBuf::from("/x/Contents/Frameworks/libndi.dylib"),
            PathBuf::from("/x/Contents/Resources/libndi.dylib"),
            PathBuf::from("/x/Contents/MacOS/libndi.dylib"),
        ]
    );
    // An exe at the filesystem root has no parent: only its own folder.
    assert_eq!(bundled_paths(TargetOs::MacOs, Path::new("/")), vec![PathBuf::from("/libndi.dylib")]);
    for os in [TargetOs::Windows, TargetOs::Linux, TargetOs::Other] {
        assert_eq!(bundled_paths(os, Path::new("/app")), vec![Path::new("/app").join(library_file_name(os))]);
    }
}

#[test]
fn the_fallback_message_points_at_a_damaged_bundle() {
    assert!(NOT_INSTALLED.contains("bundled"));
    assert!(NOT_INSTALLED.contains("Reinstall"));
    assert!(NOT_INSTALLED.contains("https://ndi.video/tools"));
}

#[test]
fn status_reports_loading_until_the_load_finishes() {
    let status = status_from(None);
    assert!(status.loading);
    assert!(!status.available);
    assert!(status.error.is_none());
}

#[test]
fn status_reports_a_failed_load() {
    let failed: Result<super::runtime::NdiRuntime, String> = Err(NOT_INSTALLED.to_string());
    let status = status_from(Some(&failed));
    assert!(!status.loading);
    assert!(!status.available);
    assert_eq!(status.error.as_deref(), Some(NOT_INSTALLED));
}

/// Machine check, not part of the normal run: loads whichever NDI runtime this computer has.
/// `cargo test --lib ndi::runtime -- --ignored --nocapture`
#[test]
#[ignore = "needs an NDI runtime installed on this machine"]
fn loads_the_runtime_installed_on_this_machine() {
    let runtime = super::runtime::NdiRuntime::load().expect("an NDI runtime should load");
    println!("loaded NDI runtime {:?} from {}", runtime.version(), runtime.path().display());
}
