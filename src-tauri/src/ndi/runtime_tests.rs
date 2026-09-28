//! Unit tests for the runtime search order (`runtime::candidate_paths`).
//! They need no NDI runtime.
use super::runtime::{candidate_paths, library_file_name, TargetOs};
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
fn env_dirs_come_first_v6_then_v5() {
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
    assert_eq!(
        paths,
        vec![
            PathBuf::from("/Library/NDI SDK for Apple/lib/macOS/libndi.dylib"),
            PathBuf::from("/usr/local/lib/libndi.dylib"),
        ]
    );
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
            Path::new("/opt/ndi").join("libndi.so.5"),
            PathBuf::from("libndi.so.6"),
            PathBuf::from("libndi.so.5"),
            PathBuf::from("/usr/lib/libndi.so"),
            Path::new("/app").join("libndi.so.6"),
        ]
    );
}

#[test]
fn exe_dir_is_last_and_duplicates_are_removed() {
    let paths = candidate_paths(
        env_of(&[("NDI_RUNTIME_DIR_V6", "/same"), ("NDI_RUNTIME_DIR_V5", "/same")]),
        TargetOs::MacOs,
        Some(Path::new("/same")),
    );
    assert_eq!(paths.first(), Some(&Path::new("/same").join("libndi.dylib")));
    assert_eq!(paths.len(), 3);
}
