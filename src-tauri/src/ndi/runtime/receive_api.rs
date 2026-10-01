//! Optional NDI® discovery and receive functions (design decision 5).
//!
//! Resolved by exported name like the send functions, but as one optional
//! group: a runtime that lacks any of them still loads and sends, and
//! `supports_receive` reports false.
use super::{resolve, NdiRuntime};
use crate::ndi::ffi::AudioFrameV3;
use crate::ndi::ffi_recv::{
    FindCreate, FindCreateV2Fn, FindDestroyFn, FindGetCurrentSourcesFn, FindInstance,
    FindWaitForSourcesFn, RecvCaptureV3Fn, RecvCreateV3, RecvCreateV3Fn, RecvDestroyFn,
    RecvFreeAudioV3Fn, RecvInstance, Source, RECV_BANDWIDTH_AUDIO_ONLY,
    RECV_COLOR_FORMAT_FASTEST,
};
use libloading::Library;
use std::ffi::{c_char, CStr};

#[derive(Clone, Copy)]
pub(super) struct ReceiveApi {
    find_create_v2: FindCreateV2Fn,
    find_destroy: FindDestroyFn,
    find_get_current_sources: FindGetCurrentSourcesFn,
    find_wait_for_sources: FindWaitForSourcesFn,
    recv_create_v3: RecvCreateV3Fn,
    recv_destroy: RecvDestroyFn,
    recv_capture_v3: RecvCaptureV3Fn,
    recv_free_audio_v3: RecvFreeAudioV3Fn,
}

impl ReceiveApi {
    /// # Safety
    /// `lib` must be an NDI runtime; the signatures in `ffi_recv` match its
    /// headers.
    pub(super) unsafe fn resolve(lib: &Library) -> Result<Self, String> {
        Ok(ReceiveApi {
            find_create_v2: resolve::<FindCreateV2Fn>(lib, b"NDIlib_find_create_v2\0")?,
            find_destroy: resolve::<FindDestroyFn>(lib, b"NDIlib_find_destroy\0")?,
            find_get_current_sources: resolve::<FindGetCurrentSourcesFn>(
                lib,
                b"NDIlib_find_get_current_sources\0",
            )?,
            find_wait_for_sources: resolve::<FindWaitForSourcesFn>(
                lib,
                b"NDIlib_find_wait_for_sources\0",
            )?,
            recv_create_v3: resolve::<RecvCreateV3Fn>(lib, b"NDIlib_recv_create_v3\0")?,
            recv_destroy: resolve::<RecvDestroyFn>(lib, b"NDIlib_recv_destroy\0")?,
            recv_capture_v3: resolve::<RecvCaptureV3Fn>(lib, b"NDIlib_recv_capture_v3\0")?,
            recv_free_audio_v3: resolve::<RecvFreeAudioV3Fn>(
                lib,
                b"NDIlib_recv_free_audio_v3\0",
            )?,
        })
    }
}

/// A NUL-terminated C string → owned UTF-8 (lossy). NULL → `None`.
///
/// # Safety
/// `ptr` must be NULL or point to a NUL-terminated string.
unsafe fn owned_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

impl NdiRuntime {
    /// The runtime exports the discovery and receive functions.
    pub fn supports_receive(&self) -> bool {
        self.api.receive.is_some()
    }

    fn receive_api(&self) -> Result<ReceiveApi, String> {
        self.api.receive.ok_or_else(|| {
            "This NDI Runtime cannot receive audio. Update NDI Tools from https://ndi.video/tools."
                .to_string()
        })
    }

    /// Create a finder that also lists sources on this machine.
    pub fn find_create(&self) -> Result<FindInstance, String> {
        let api = self.receive_api()?;
        let settings = FindCreate {
            show_local_sources: true,
            p_groups: std::ptr::null(),
            p_extra_ips: std::ptr::null(),
        };
        // SAFETY: `settings` is valid for the call; the SDK copies it.
        let instance = unsafe { (api.find_create_v2)(&settings) };
        if instance.is_null() {
            Err("NDI could not start looking for sources".to_string())
        } else {
            Ok(instance)
        }
    }

    /// Wait up to `timeout_ms` for the source list to change.
    ///
    /// # Safety
    /// `instance` must come from `find_create` and not be destroyed yet.
    pub unsafe fn find_wait(&self, instance: FindInstance, timeout_ms: u32) -> bool {
        match self.api.receive {
            Some(api) => (api.find_wait_for_sources)(instance, timeout_ms),
            None => false,
        }
    }

    /// The sources found so far, copied: `(name, url address)`.
    ///
    /// # Safety
    /// `instance` must come from `find_create` and not be destroyed yet.
    pub unsafe fn find_sources(&self, instance: FindInstance) -> Vec<(String, Option<String>)> {
        let Some(api) = self.api.receive else {
            return Vec::new();
        };
        let mut count: u32 = 0;
        let list = (api.find_get_current_sources)(instance, &mut count);
        if list.is_null() || count == 0 {
            return Vec::new();
        }
        // The array stays valid until the next call on this finder or its
        // destruction; everything is copied out before returning.
        let sources: &[Source] = std::slice::from_raw_parts(list, count as usize);
        let mut copied = Vec::with_capacity(sources.len());
        for source in sources {
            if let Some(name) = owned_string(source.p_ndi_name) {
                copied.push((name, owned_string(source.p_url_address)));
            }
        }
        copied
    }

    /// # Safety
    /// `instance` must come from `find_create`; it must not be used again.
    pub unsafe fn find_destroy(&self, instance: FindInstance) {
        if let Some(api) = self.api.receive {
            (api.find_destroy)(instance);
        }
    }

    /// Create an audio-only receiver named `recv_name`, connected to the
    /// source called `source_name`.
    pub fn recv_create(&self, source_name: &CStr, recv_name: &CStr) -> Result<RecvInstance, String> {
        let api = self.receive_api()?;
        let settings = RecvCreateV3 {
            source_to_connect_to: Source {
                p_ndi_name: source_name.as_ptr(),
                p_url_address: std::ptr::null(),
            },
            color_format: RECV_COLOR_FORMAT_FASTEST,
            bandwidth: RECV_BANDWIDTH_AUDIO_ONLY,
            allow_video_fields: false,
            p_ndi_recv_name: recv_name.as_ptr(),
        };
        // SAFETY: `settings` and both strings are valid for the call; the
        // SDK copies what it keeps.
        let instance = unsafe { (api.recv_create_v3)(&settings) };
        if instance.is_null() {
            Err(format!(
                "NDI could not create a receiver for '{}'",
                source_name.to_string_lossy()
            ))
        } else {
            Ok(instance)
        }
    }

    /// Wait up to `timeout_ms` for an audio frame; returns the
    /// `NDIlib_frame_type_e`. Only audio is asked for (video and metadata
    /// are NULL). An audio frame must be released with `recv_free_audio`.
    ///
    /// # Safety
    /// `instance` must come from `recv_create`, not be destroyed yet and be
    /// used by one thread at a time.
    pub unsafe fn recv_capture_audio(
        &self,
        instance: RecvInstance,
        frame: &mut AudioFrameV3,
        timeout_ms: u32,
    ) -> i32 {
        match self.api.receive {
            Some(api) => (api.recv_capture_v3)(
                instance,
                std::ptr::null_mut(),
                frame,
                std::ptr::null_mut(),
                timeout_ms,
            ),
            None => crate::ndi::ffi_recv::FRAME_TYPE_ERROR,
        }
    }

    /// # Safety
    /// `frame` must have been filled by `recv_capture_audio` on `instance`
    /// and not freed yet.
    pub unsafe fn recv_free_audio(&self, instance: RecvInstance, frame: &AudioFrameV3) {
        if let Some(api) = self.api.receive {
            (api.recv_free_audio_v3)(instance, frame);
        }
    }

    /// # Safety
    /// `instance` must come from `recv_create`; it must not be used again.
    pub unsafe fn recv_destroy(&self, instance: RecvInstance) {
        if let Some(api) = self.api.receive {
            (api.recv_destroy)(instance);
        }
    }
}
