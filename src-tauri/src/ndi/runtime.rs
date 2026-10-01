//! NDI® runtime loader.
//!
//! Nothing from NDI is linked at build time (design decision 2 in
//! `docs/specs/2026-09-28-integrations-design.md`). The installed NDI Runtime
//! library is opened with `libloading` once, on a background thread started
//! at app setup (`preload`) or on first use, and each
//! function is resolved by its exported name. The `NDIlib_v5_load` function
//! table is deliberately NOT used: a struct whose field order we got wrong
//! would call the wrong function and crash.
//!
//! The C types and function signatures are in `ffi`.
use super::ffi::{
    AudioFrameV3, DestroyFn, InitializeFn, SendAudioV3Fn, SendCreate, SendCreateFn,
    SendDestroyFn, SendInstance, VersionFn,
};
use libloading::Library;
use receive_api::ReceiveApi;
use serde::Serialize;
use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod receive_api;

/// Shown when no runtime could be loaded.
pub const NOT_INSTALLED: &str =
    "NDI Runtime not installed. Install NDI Tools from https://ndi.video/tools, then restart TIWATON AI Studio.";

#[derive(Clone, Copy)]
struct NdiApi {
    initialize: InitializeFn,
    destroy: DestroyFn,
    version: VersionFn,
    send_create: SendCreateFn,
    send_destroy: SendDestroyFn,
    send_audio_v3: SendAudioV3Fn,
    /// Discovery and receive (NDI in). Optional: `None` still sends.
    receive: Option<ReceiveApi>,
}

// ── Search order ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetOs {
    Windows,
    MacOs,
    Linux,
    Other,
}

impl TargetOs {
    pub fn current() -> Self {
        if cfg!(windows) {
            TargetOs::Windows
        } else if cfg!(target_os = "macos") {
            TargetOs::MacOs
        } else if cfg!(target_os = "linux") {
            TargetOs::Linux
        } else {
            TargetOs::Other
        }
    }
}

/// File name of the runtime library on `os`.
pub fn library_file_name(os: TargetOs) -> &'static str {
    match os {
        TargetOs::Windows if cfg!(target_pointer_width = "32") => "Processing.NDI.Lib.x86.dll",
        TargetOs::Windows => "Processing.NDI.Lib.x64.dll",
        TargetOs::MacOs => "libndi.dylib",
        TargetOs::Linux => "libndi.so.6",
        TargetOs::Other => "libndi.so",
    }
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

/// Where to look for the runtime, in order:
/// 1. `NDI_RUNTIME_DIR_V6`, 2. `NDI_RUNTIME_DIR_V5` (set by the installers),
/// 3. the default install locations for `os`,
/// 4. `exe_dir`, the app's own folder.
///
/// Pure: the environment and the executable folder are passed in, so this
/// is unit-tested without touching the machine.
pub fn candidate_paths(
    env: impl Fn(&str) -> Option<String>,
    os: TargetOs,
    exe_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let lib = library_file_name(os);
    let mut paths: Vec<PathBuf> = Vec::new();

    for (var, linux_name) in [
        ("NDI_RUNTIME_DIR_V6", "libndi.so.6"),
        ("NDI_RUNTIME_DIR_V5", "libndi.so.5"),
    ] {
        let Some(dir) = env(var) else { continue };
        let dir = dir.trim();
        if dir.is_empty() {
            continue;
        }
        let name = if os == TargetOs::Linux { linux_name } else { lib };
        push_unique(&mut paths, Path::new(dir).join(name));
    }

    match os {
        TargetOs::Windows => {
            push_unique(
                &mut paths,
                PathBuf::from(format!(r"C:\Program Files\NDI\NDI 6 Runtime\v6\{lib}")),
            );
            push_unique(
                &mut paths,
                PathBuf::from(format!(r"C:\Program Files\NDI\NDI 5 Runtime\v5\{lib}")),
            );
        }
        TargetOs::MacOs => {
            push_unique(
                &mut paths,
                PathBuf::from("/Library/NDI SDK for Apple/lib/macOS/libndi.dylib"),
            );
            push_unique(&mut paths, PathBuf::from("/usr/local/lib/libndi.dylib"));
        }
        TargetOs::Linux => {
            // Bare names: the dynamic loader searches its usual paths.
            push_unique(&mut paths, PathBuf::from("libndi.so.6"));
            push_unique(&mut paths, PathBuf::from("libndi.so.5"));
            push_unique(&mut paths, PathBuf::from("/usr/lib/libndi.so"));
        }
        TargetOs::Other => {}
    }

    if let Some(dir) = exe_dir {
        push_unique(&mut paths, dir.join(lib));
    }
    paths
}

// ── Runtime ──────────────────────────────────────────────────────────────────

/// A loaded, initialised NDI runtime.
///
/// `Send + Sync` (checked by the `RUNTIME` static, which requires both): it
/// holds only plain function pointers, strings and the `Library`, which
/// libloading marks `Send + Sync`. Sharing is sound because the NDI SDK
/// documents its library functions as thread-safe; each send instance is
/// created, used and destroyed on a single sender thread.
pub struct NdiRuntime {
    api: NdiApi,
    version: Option<String>,
    path: PathBuf,
    /// Keeps the library mapped for as long as the function pointers in
    /// `api` are used. Declared last so it is dropped last.
    _lib: Library,
}

/// # Safety
/// `T` must be the exact function-pointer type of the exported symbol, and
/// `name` must be NUL-terminated.
unsafe fn resolve<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, String> {
    lib.get::<T>(name).map(|symbol| *symbol).map_err(|e| {
        let printable = String::from_utf8_lossy(name);
        format!("missing {}: {e}", printable.trim_end_matches('\0'))
    })
}

impl NdiApi {
    /// # Safety
    /// `lib` must be an NDI runtime; the signatures above match its headers.
    unsafe fn resolve_all(lib: &Library) -> Result<Self, String> {
        Ok(NdiApi {
            initialize: resolve::<InitializeFn>(lib, b"NDIlib_initialize\0")?,
            destroy: resolve::<DestroyFn>(lib, b"NDIlib_destroy\0")?,
            version: resolve::<VersionFn>(lib, b"NDIlib_version\0")?,
            send_create: resolve::<SendCreateFn>(lib, b"NDIlib_send_create\0")?,
            send_destroy: resolve::<SendDestroyFn>(lib, b"NDIlib_send_destroy\0")?,
            send_audio_v3: resolve::<SendAudioV3Fn>(lib, b"NDIlib_send_send_audio_v3\0")?,
            // Optional: a runtime without the receive group still sends.
            receive: match ReceiveApi::resolve(lib) {
                Ok(api) => Some(api),
                Err(err) => {
                    log::warn!("NDI receive unavailable: {err}");
                    None
                }
            },
        })
    }
}

impl NdiRuntime {
    /// Open the first candidate that loads and exports every function we
    /// need, then call `NDIlib_initialize`.
    pub fn load() -> Result<NdiRuntime, String> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let candidates = candidate_paths(
            |key| std::env::var(key).ok(),
            TargetOs::current(),
            exe_dir.as_deref(),
        );
        for path in &candidates {
            match Self::open(path) {
                Ok(runtime) => {
                    log::info!(
                        "NDI runtime {} loaded from {}",
                        runtime.version.as_deref().unwrap_or("(unknown version)"),
                        path.display()
                    );
                    return Ok(runtime);
                }
                Err(err) => log::debug!("NDI runtime not loaded from {}: {err}", path.display()),
            }
        }
        Err(NOT_INSTALLED.to_string())
    }

    fn open(path: &Path) -> Result<NdiRuntime, String> {
        // SAFETY: loading a library runs its initialisers. The candidates are
        // the NDI Runtime's own install locations (or ones the operator set
        // in the NDI_RUNTIME_DIR_* variables).
        let lib = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
        // SAFETY: the function types match the SDK headers (see above).
        let api = unsafe { NdiApi::resolve_all(&lib) }?;
        // SAFETY: `NDIlib_initialize` takes no arguments; it returns false
        // when the CPU is not supported.
        let initialised = unsafe { (api.initialize)() };
        if !initialised {
            return Err("NDIlib_initialize failed: this CPU is not supported by NDI".to_string());
        }
        // SAFETY: returns a static NUL-terminated string (or NULL).
        let version_ptr = unsafe { (api.version)() };
        let version = if version_ptr.is_null() {
            None
        } else {
            // SAFETY: checked non-null; the SDK owns the static string.
            Some(unsafe { CStr::from_ptr(version_ptr) }.to_string_lossy().into_owned())
        };
        Ok(NdiRuntime {
            api,
            version,
            path: path.to_path_buf(),
            _lib: lib,
        })
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Create an audio sender named `name`, with `clock_audio` on so that
    /// `send_audio` paces its caller in real time.
    pub fn send_create(&self, name: &CStr) -> Result<SendInstance, String> {
        let settings = SendCreate {
            p_ndi_name: name.as_ptr(),
            p_groups: std::ptr::null(),
            clock_video: false,
            clock_audio: true,
        };
        // SAFETY: `settings` and `name` are valid for the duration of the call;
        // the SDK copies what it keeps.
        let instance = unsafe { (self.api.send_create)(&settings) };
        if instance.is_null() {
            Err(format!("NDI could not create the sender '{}'", name.to_string_lossy()))
        } else {
            Ok(instance)
        }
    }

    /// # Safety
    /// `instance` must come from `send_create` and not be destroyed yet, and
    /// only one thread may use it at a time. `frame.p_data` must point to
    /// `no_channels × channel_stride_in_bytes` readable bytes.
    pub unsafe fn send_audio(&self, instance: SendInstance, frame: &AudioFrameV3) {
        (self.api.send_audio_v3)(instance, frame);
    }

    /// # Safety
    /// `instance` must come from `send_create`; it must not be used again.
    pub unsafe fn send_destroy(&self, instance: SendInstance) {
        (self.api.send_destroy)(instance);
    }
}

impl Drop for NdiRuntime {
    fn drop(&mut self) {
        // SAFETY: balances the successful `NDIlib_initialize` in `open`.
        // The library is still mapped: fields are dropped after this runs.
        unsafe { (self.api.destroy)() };
    }
}

/// Loaded lazily, once, and kept for the life of the process.
static RUNTIME: OnceLock<Result<NdiRuntime, String>> = OnceLock::new();

/// Start loading the runtime on a background thread (called at app setup),
/// so `ndi_status` never waits for the library to load.
pub fn preload() {
    let spawned = std::thread::Builder::new()
        .name("ndi-runtime-load".to_string())
        .spawn(|| {
            let _ = runtime();
        });
    if let Err(err) = spawned {
        log::warn!("cannot start the NDI runtime loader thread: {err}");
    }
}

/// The shared runtime, loading it on first use. Blocks while another thread
/// is loading it.
pub fn runtime() -> Result<&'static NdiRuntime, String> {
    match RUNTIME.get_or_init(NdiRuntime::load) {
        Ok(runtime) => Ok(runtime),
        Err(err) => Err(err.clone()),
    }
}

/// Runtime availability for the UI (`ndi_status`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NdiStatus {
    pub available: bool,
    /// The background load (`preload`) has not finished yet.
    pub loading: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub error: Option<String>,
    /// The runtime can also discover and receive sources (NDI in).
    pub receive: bool,
}

/// Never blocks: reports `loading` until the background load is done.
pub fn ndi_status() -> NdiStatus {
    status_from(RUNTIME.get())
}

/// `None` = still loading.
pub(crate) fn status_from(slot: Option<&Result<NdiRuntime, String>>) -> NdiStatus {
    match slot {
        None => NdiStatus {
            available: false,
            loading: true,
            version: None,
            path: None,
            error: None,
            receive: false,
        },
        Some(Ok(runtime)) => NdiStatus {
            available: true,
            loading: false,
            version: runtime.version().map(str::to_string),
            path: Some(runtime.path().display().to_string()),
            error: None,
            receive: runtime.supports_receive(),
        },
        Some(Err(err)) => NdiStatus {
            available: false,
            loading: false,
            version: None,
            path: None,
            error: Some(err.clone()),
            receive: false,
        },
    }
}
