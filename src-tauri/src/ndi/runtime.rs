//! NDI® runtime loader.
//!
//! Nothing from NDI is linked at build time (design decision 2 in
//! `docs/specs/2026-09-28-integrations-design.md`). The installed NDI Runtime
//! library is opened with `libloading` the first time it is needed, and each
//! function is resolved by its exported name. The `NDIlib_v5_load` function
//! table is deliberately NOT used: a struct whose field order we got wrong
//! would call the wrong function and crash.
//!
//! Only the structs and functions the audio sender needs are declared here,
//! with the exact field order of the NDI SDK headers
//! (`Processing.NDI.structs.h`, `Processing.NDI.Send.h`,
//! `Processing.NDI.Lib.h`).
use libloading::Library;
use serde::Serialize;
use std::ffi::{c_char, c_void, CStr};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Shown when no runtime could be loaded.
pub const NOT_INSTALLED: &str =
    "NDI Runtime not installed. Install NDI Tools from https://ndi.video/tools, then restart TIWATON AI Studio.";

// ── SDK types ────────────────────────────────────────────────────────────────

/// `NDIlib_send_instance_t`: an opaque `struct NDIlib_send_instance_type*`.
pub type SendInstance = *mut c_void;

/// `NDIlib_send_timecode_synthesize` (`Processing.NDI.structs.h`): ask the
/// SDK to generate the timecode from the sample count.
pub const SEND_TIMECODE_SYNTHESIZE: i64 = i64::MAX;

/// `NDIlib_FourCC_audio_type_FLTP = NDI_LIB_FOURCC('F', 'L', 'T', 'p')`:
/// planar 32-bit float. `NDI_LIB_FOURCC(a,b,c,d)` is
/// `a | b << 8 | c << 16 | d << 24`.
pub const FOURCC_AUDIO_FLTP: u32 =
    (b'F' as u32) | ((b'L' as u32) << 8) | ((b'T' as u32) << 16) | ((b'p' as u32) << 24);

/// `NDIlib_send_create_t` (`Processing.NDI.Send.h`), same field order.
#[repr(C)]
pub struct SendCreate {
    /// `const char* p_ndi_name`: the source name (UTF-8, NUL-terminated).
    pub p_ndi_name: *const c_char,
    /// `const char* p_groups`: groups to announce in; NULL = default group.
    pub p_groups: *const c_char,
    /// `bool clock_video`: pace `send_video` calls to the frame rate. C
    /// `bool` is one byte, the same as Rust `bool`.
    pub clock_video: bool,
    /// `bool clock_audio`: `send_audio` blocks so that audio is sent in real
    /// time. This is what paces the sender thread.
    pub clock_audio: bool,
}

/// `NDIlib_audio_frame_v3_t` (`Processing.NDI.structs.h`), same field order.
#[repr(C)]
pub struct AudioFrameV3 {
    /// `int sample_rate`: samples per second, e.g. 48000.
    pub sample_rate: i32,
    /// `int no_channels`: number of audio channels.
    pub no_channels: i32,
    /// `int no_samples`: samples per channel in this frame.
    pub no_samples: i32,
    /// `int64_t timecode`: 100 ns units, or `NDIlib_send_timecode_synthesize`.
    pub timecode: i64,
    /// `NDIlib_FourCC_audio_type_e FourCC`: an int-sized C enum; we only
    /// send `NDIlib_FourCC_audio_type_FLTP`.
    pub four_cc: u32,
    /// `uint8_t* p_data`: for FLTP, channel 0's samples, then channel 1's,
    /// each `channel_stride_in_bytes` apart. The SDK only reads it.
    pub p_data: *mut u8,
    /// `union { int channel_stride_in_bytes; int data_size_in_bytes; }`:
    /// for FLTP this is the stride between channel planes, in bytes.
    pub channel_stride_in_bytes: i32,
    /// `const char* p_metadata`: optional XML metadata; NULL for none.
    pub p_metadata: *const c_char,
    /// `int64_t timestamp`: filled in by the SDK on receive; 0 on send.
    pub timestamp: i64,
}

impl AudioFrameV3 {
    /// A planar-float frame over `planar` (`channels` planes of `samples`
    /// each) with synthesized timecode. `planar` must outlive the send call.
    pub fn fltp(sample_rate: u32, channels: usize, samples: usize, planar: &[f32]) -> Self {
        debug_assert!(planar.len() >= channels * samples);
        AudioFrameV3 {
            sample_rate: sample_rate as i32,
            no_channels: channels as i32,
            no_samples: samples as i32,
            timecode: SEND_TIMECODE_SYNTHESIZE,
            four_cc: FOURCC_AUDIO_FLTP,
            p_data: planar.as_ptr() as *const u8 as *mut u8,
            channel_stride_in_bytes: (samples * std::mem::size_of::<f32>()) as i32,
            p_metadata: std::ptr::null(),
            timestamp: 0,
        }
    }
}

// Function signatures, from `Processing.NDI.Lib.h` and `Processing.NDI.Send.h`.
// The SDK declares no calling convention, so the platform C ABI applies.
/// `bool NDIlib_initialize(void)`
type InitializeFn = unsafe extern "C" fn() -> bool;
/// `void NDIlib_destroy(void)`
type DestroyFn = unsafe extern "C" fn();
/// `const char* NDIlib_version(void)`
type VersionFn = unsafe extern "C" fn() -> *const c_char;
/// `NDIlib_send_instance_t NDIlib_send_create(const NDIlib_send_create_t*)`
type SendCreateFn = unsafe extern "C" fn(*const SendCreate) -> SendInstance;
/// `void NDIlib_send_destroy(NDIlib_send_instance_t)`
type SendDestroyFn = unsafe extern "C" fn(SendInstance);
/// `void NDIlib_send_send_audio_v3(NDIlib_send_instance_t, const NDIlib_audio_frame_v3_t*)`
type SendAudioV3Fn = unsafe extern "C" fn(SendInstance, *const AudioFrameV3);

#[derive(Clone, Copy)]
struct NdiApi {
    initialize: InitializeFn,
    destroy: DestroyFn,
    version: VersionFn,
    send_create: SendCreateFn,
    send_destroy: SendDestroyFn,
    send_audio_v3: SendAudioV3Fn,
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

/// The shared runtime, loading it on first use.
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
    pub version: Option<String>,
    pub path: Option<String>,
    pub error: Option<String>,
}

pub fn ndi_status() -> NdiStatus {
    match runtime() {
        Ok(runtime) => NdiStatus {
            available: true,
            version: runtime.version().map(str::to_string),
            path: Some(runtime.path().display().to_string()),
            error: None,
        },
        Err(err) => NdiStatus {
            available: false,
            version: None,
            path: None,
            error: Some(err),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn fltp_fourcc_matches_the_sdk_value() {
        // NDI_LIB_FOURCC('F','L','T','p') = 0x70544C46.
        assert_eq!(FOURCC_AUDIO_FLTP, 0x7054_4C46);
    }

    #[test]
    fn struct_layouts_follow_the_c_headers() {
        use std::mem::{align_of, size_of};
        let ptr = size_of::<*const u8>();
        // Two pointers then two one-byte bools, padded to pointer alignment.
        assert_eq!(size_of::<SendCreate>(), 2 * ptr + align_of::<*const u8>());
        if ptr == 8 {
            // 3×i32 (+4 pad), i64, u32 (+4), ptr, i32 (+4), ptr, i64.
            assert_eq!(size_of::<AudioFrameV3>(), 64);
        }
    }

    #[test]
    fn fltp_frame_describes_planar_buffer() {
        let planar = vec![0.0f32; 2 * 480];
        let frame = AudioFrameV3::fltp(48_000, 2, 480, &planar);
        assert_eq!(frame.no_channels, 2);
        assert_eq!(frame.no_samples, 480);
        assert_eq!(frame.channel_stride_in_bytes, 480 * 4);
        assert_eq!(frame.timecode, SEND_TIMECODE_SYNTHESIZE);
        assert_eq!(frame.p_data as usize, planar.as_ptr() as usize);
    }
}
