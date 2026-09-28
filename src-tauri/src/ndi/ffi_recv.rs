//! Hand-declared NDI® SDK types for discovery and receiving, with the exact
//! field order of the NDI SDK 6 headers (`Processing.NDI.structs.h`,
//! `Processing.NDI.Find.h`, `Processing.NDI.Recv.h`). The functions are
//! resolved by name, and optionally, in `runtime::receive_api`: a runtime
//! without them still sends.
//!
//! C enums in these headers end with a `0x7fffffff` sentinel, so they are
//! `int`-sized; they are declared as `i32` here.
use super::ffi::AudioFrameV3;
use std::ffi::{c_char, c_void};

/// `NDIlib_find_instance_t`: an opaque `struct NDIlib_find_instance_type*`.
pub type FindInstance = *mut c_void;
/// `NDIlib_recv_instance_t`: an opaque `struct NDIlib_recv_instance_type*`.
pub type RecvInstance = *mut c_void;

/// `NDIlib_frame_type_e` values (`Processing.NDI.structs.h`) that the
/// receiver acts on; `none` (0, a timeout) and the status/source-change
/// values (100, 101) need no action.
pub const FRAME_TYPE_AUDIO: i32 = 2;
pub const FRAME_TYPE_ERROR: i32 = 4;

/// `NDIlib_recv_bandwidth_audio_only = 10`: metadata and audio, no video.
pub const RECV_BANDWIDTH_AUDIO_ONLY: i32 = 10;
/// `NDIlib_recv_color_format_fastest = 100`. No video is received with
/// audio-only bandwidth, so the colour format is never used.
pub const RECV_COLOR_FORMAT_FASTEST: i32 = 100;

/// `NDIlib_source_t` (`Processing.NDI.structs.h`), same field order.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Source {
    /// `const char* p_ndi_name`: "MACHINE (Source name)", UTF-8.
    pub p_ndi_name: *const c_char,
    /// `union { const char* p_url_address; const char* p_ip_address; }`:
    /// both members are one pointer, so the union is one pointer.
    pub p_url_address: *const c_char,
}

/// `NDIlib_find_create_t` (`Processing.NDI.Find.h`), same field order.
#[repr(C)]
pub struct FindCreate {
    /// `bool show_local_sources`: include sources on this machine.
    pub show_local_sources: bool,
    /// `const char* p_groups`: NULL = the default groups.
    pub p_groups: *const c_char,
    /// `const char* p_extra_ips`: NULL = none.
    pub p_extra_ips: *const c_char,
}

/// `NDIlib_recv_create_v3_t` (`Processing.NDI.Recv.h`), same field order.
#[repr(C)]
pub struct RecvCreateV3 {
    /// `NDIlib_source_t source_to_connect_to`, held by value.
    pub source_to_connect_to: Source,
    /// `NDIlib_recv_color_format_e color_format`.
    pub color_format: i32,
    /// `NDIlib_recv_bandwidth_e bandwidth`.
    pub bandwidth: i32,
    /// `bool allow_video_fields`.
    pub allow_video_fields: bool,
    /// `const char* p_ndi_recv_name`: this receiver's name.
    pub p_ndi_recv_name: *const c_char,
}

/// An all-zero audio frame for `NDIlib_recv_capture_v3` to fill in.
pub fn empty_audio_frame() -> AudioFrameV3 {
    AudioFrameV3 {
        sample_rate: 0,
        no_channels: 0,
        no_samples: 0,
        timecode: 0,
        four_cc: 0,
        p_data: std::ptr::null_mut(),
        channel_stride_in_bytes: 0,
        p_metadata: std::ptr::null(),
        timestamp: 0,
    }
}

// Function signatures, from `Processing.NDI.Find.h` and `Processing.NDI.Recv.h`.
/// `NDIlib_find_instance_t NDIlib_find_create_v2(const NDIlib_find_create_t*)`
pub type FindCreateV2Fn = unsafe extern "C" fn(*const FindCreate) -> FindInstance;
/// `void NDIlib_find_destroy(NDIlib_find_instance_t)`
pub type FindDestroyFn = unsafe extern "C" fn(FindInstance);
/// `const NDIlib_source_t* NDIlib_find_get_current_sources(NDIlib_find_instance_t, uint32_t*)`
pub type FindGetCurrentSourcesFn = unsafe extern "C" fn(FindInstance, *mut u32) -> *const Source;
/// `bool NDIlib_find_wait_for_sources(NDIlib_find_instance_t, uint32_t timeout_in_ms)`
pub type FindWaitForSourcesFn = unsafe extern "C" fn(FindInstance, u32) -> bool;
/// `NDIlib_recv_instance_t NDIlib_recv_create_v3(const NDIlib_recv_create_v3_t*)`
pub type RecvCreateV3Fn = unsafe extern "C" fn(*const RecvCreateV3) -> RecvInstance;
/// `void NDIlib_recv_destroy(NDIlib_recv_instance_t)`
pub type RecvDestroyFn = unsafe extern "C" fn(RecvInstance);
/// `NDIlib_frame_type_e NDIlib_recv_capture_v3(NDIlib_recv_instance_t,
/// NDIlib_video_frame_v2_t*, NDIlib_audio_frame_v3_t*, NDIlib_metadata_frame_t*,
/// uint32_t timeout_in_ms)`. The video and metadata pointers are always NULL
/// here, so their struct types are not declared.
pub type RecvCaptureV3Fn = unsafe extern "C" fn(
    RecvInstance,
    *mut c_void,
    *mut AudioFrameV3,
    *mut c_void,
    u32,
) -> i32;
/// `void NDIlib_recv_free_audio_v3(NDIlib_recv_instance_t, const NDIlib_audio_frame_v3_t*)`
pub type RecvFreeAudioV3Fn = unsafe extern "C" fn(RecvInstance, *const AudioFrameV3);

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn struct_layouts_follow_the_c_headers() {
        let ptr = size_of::<*const u8>();
        assert_eq!(size_of::<Source>(), 2 * ptr);
        // bool (+pad to pointer), two pointers.
        assert_eq!(size_of::<FindCreate>(), 3 * ptr);
        if ptr == 8 {
            // Source (16), i32, i32, bool (+7 pad), ptr.
            assert_eq!(size_of::<RecvCreateV3>(), 40);
        }
        assert_eq!(align_of::<RecvCreateV3>(), align_of::<*const u8>());
    }

    #[test]
    fn enum_values_match_the_headers() {
        assert_eq!(FRAME_TYPE_AUDIO, 2);
        assert_eq!(FRAME_TYPE_ERROR, 4);
        assert_eq!(RECV_BANDWIDTH_AUDIO_ONLY, 10);
    }

    #[test]
    fn empty_frame_has_no_data() {
        let frame = empty_audio_frame();
        assert!(frame.p_data.is_null());
        assert_eq!(frame.no_samples, 0);
    }
}
