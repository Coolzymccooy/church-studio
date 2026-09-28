//! Hand-declared NDI® SDK types: only what the audio sender needs, with the
//! exact field order of the NDI SDK headers (`Processing.NDI.structs.h`,
//! `Processing.NDI.Send.h`, `Processing.NDI.Lib.h`). The functions are
//! resolved by name in `runtime`.
use std::ffi::{c_char, c_void};

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
pub type InitializeFn = unsafe extern "C" fn() -> bool;
/// `void NDIlib_destroy(void)`
pub type DestroyFn = unsafe extern "C" fn();
/// `const char* NDIlib_version(void)`
pub type VersionFn = unsafe extern "C" fn() -> *const c_char;
/// `NDIlib_send_instance_t NDIlib_send_create(const NDIlib_send_create_t*)`
pub type SendCreateFn = unsafe extern "C" fn(*const SendCreate) -> SendInstance;
/// `void NDIlib_send_destroy(NDIlib_send_instance_t)`
pub type SendDestroyFn = unsafe extern "C" fn(SendInstance);
/// `void NDIlib_send_send_audio_v3(NDIlib_send_instance_t, const NDIlib_audio_frame_v3_t*)`
pub type SendAudioV3Fn = unsafe extern "C" fn(SendInstance, *const AudioFrameV3);

#[cfg(test)]
mod tests {
    use super::*;

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
