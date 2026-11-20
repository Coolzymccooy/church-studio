//! macOS computer audio: a Core Audio process tap (macOS 14.4+).
//!
//! cpal's own loopback taps every process, this one included, and leaves the
//! sound playing; here the tap:
//! - leaves this process out, so the Monitor, Stream and Main buses never
//!   feed back in, whatever devices they play on;
//! - mutes the tapped apps at their normal output while it runs
//!   (`MutedWhenTapped`), so the music is heard once, through the mixer.
//!
//! The tap is read through a private aggregate device with an IOProc that
//! hands each buffer to the `CaptureWriter`. `ProcessTap` stops the IOProc
//! and destroys the aggregate device and the tap when it is dropped (engine
//! stop, a failed start, or unwinding after a panic). Both are private to
//! this process, so Core Audio removes them too if the process dies.
//!
//! Permission: the "System Audio Recording" privacy setting
//! (`NSAudioCaptureUsageDescription`). A denied tap delivers silence instead
//! of failing, so `start` checks first and refuses with a clear message.
use super::capture::CaptureWriter;
use objc2::rc::Retained;
use objc2::AnyThread;
use objc2_core_audio::{
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioHardwarePropertyTranslatePIDToProcessObject,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioObjectUnknown, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
    kAudioTapPropertyFormat, AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID,
    AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
    AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
    AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectID,
    AudioObjectPropertyAddress, CATapDescription, CATapMuteBehavior,
};
use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatLinearPCM, AudioBuffer, AudioBufferList,
    AudioStreamBasicDescription, AudioTimeStamp,
};
use objc2_core_foundation::{
    kCFAllocatorDefault, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks,
    kCFTypeDictionaryValueCallBacks, CFArray, CFBoolean, CFDictionary, CFMutableDictionary,
    CFRetained, CFString, CFType,
};
use objc2_foundation::{NSArray, NSNumber, NSString};
use ringbuf::HeapProd;
use std::ffi::{c_void, CStr};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

/// Shown when macOS has not allowed this app to record other apps' sound.
pub const PERMISSION_DENIED: &str = "Computer audio needs permission. Open System Settings → Privacy & Security → Screen & System Audio Recording, allow TIWATON AI Studio under System Audio Recording Only, then start the engine again. Or turn computer audio off.";

static INSTANCE: AtomicU32 = AtomicU32::new(0);

type Writer = CaptureWriter<HeapProd<f32>>;

/// The IOProc's state, owned by `ProcessTap` and only touched by the
/// aggregate device's IO thread while the IOProc runs.
struct IoContext {
    writer: Writer,
}

pub struct ProcessTap {
    tap: AudioObjectID,
    aggregate: AudioObjectID,
    io_proc: AudioDeviceIOProcID,
    started: bool,
    context: *mut IoContext,
    capture_rate: u32,
}

impl ProcessTap {
    /// Tap every process but this one and start reading it. `make_writer`
    /// gets the tap's sample rate.
    pub fn start(make_writer: impl FnOnce(u32) -> Writer) -> Result<Self, String> {
        if permission() == Permission::Denied {
            return Err(PERMISSION_DENIED.to_string());
        }
        let own = own_process_object()?;
        let mut tap = ProcessTap {
            tap: kAudioObjectUnknown,
            aggregate: kAudioObjectUnknown,
            io_proc: None,
            started: false,
            context: std::ptr::null_mut(),
            capture_rate: 0,
        };
        // Every early return below drops `tap`, undoing what was created.
        let pid = std::process::id();
        let instance = INSTANCE.fetch_add(1, Ordering::Relaxed);
        let description = tap_description(own, pid, instance);
        let mut tap_id = kAudioObjectUnknown;
        check(
            unsafe { AudioHardwareCreateProcessTap(Some(&description), &mut tap_id) },
            "create the system audio tap",
        )?;
        tap.tap = tap_id;
        let format = tap_format(tap_id)?;
        tap.capture_rate = format.mSampleRate.round() as u32;

        let tap_uid = unsafe { description.UUID().UUIDString() };
        let properties = aggregate_properties(
            &tap_uid,
            &format!("com.tiwaton.studio.computer-audio.{pid}.{instance}"),
            "TIWATON computer audio",
        );
        let mut aggregate = kAudioObjectUnknown;
        check(
            unsafe {
                AudioHardwareCreateAggregateDevice(&properties, NonNull::from(&mut aggregate))
            },
            "create the computer audio device",
        )?;
        tap.aggregate = aggregate;

        tap.context = Box::into_raw(Box::new(IoContext {
            writer: make_writer(tap.capture_rate),
        }));
        let mut io_proc: AudioDeviceIOProcID = None;
        check(
            unsafe {
                AudioDeviceCreateIOProcID(
                    aggregate,
                    Some(io_proc_callback),
                    tap.context.cast(),
                    NonNull::from(&mut io_proc),
                )
            },
            "attach to the computer audio device",
        )?;
        tap.io_proc = io_proc;
        check(
            unsafe { AudioDeviceStart(aggregate, io_proc) },
            "start the computer audio device",
        )?;
        tap.started = true;
        Ok(tap)
    }

    pub fn capture_rate(&self) -> u32 {
        self.capture_rate
    }
}

impl Drop for ProcessTap {
    /// Undo whatever `start` created, in reverse order. Statuses are not
    /// checked: there is nothing better to do with a failure here.
    fn drop(&mut self) {
        unsafe {
            if self.started {
                let _ = AudioDeviceStop(self.aggregate, self.io_proc);
            }
            if self.io_proc.is_some() {
                let _ = AudioDeviceDestroyIOProcID(self.aggregate, self.io_proc);
            }
            if self.aggregate != kAudioObjectUnknown {
                let _ = AudioHardwareDestroyAggregateDevice(self.aggregate);
            }
            if self.tap != kAudioObjectUnknown {
                let _ = AudioHardwareDestroyProcessTap(self.tap);
            }
            // The IOProc is stopped and destroyed, so nothing uses it now.
            if !self.context.is_null() {
                drop(Box::from_raw(self.context));
            }
        }
    }
}

/// IOProc of the aggregate device: hand the tap's input to the writer.
/// Real-time safe (the writer is), and a panic never unwinds into Core Audio.
unsafe extern "C-unwind" fn io_proc_callback(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    if client.is_null() {
        return 0;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: `client` is the `IoContext` this IOProc was created with;
        // it outlives the IOProc and only this IO thread touches it.
        let context = unsafe { &mut *client.cast::<IoContext>() };
        // SAFETY: Core Audio passes a valid list for the duration of the call.
        let buffers = unsafe { buffers(input) };
        match buffers {
            [] => {}
            [one] => {
                if let Some(samples) = unsafe { samples(one) } {
                    context
                        .writer
                        .write_interleaved(samples, one.mNumberChannels.max(1) as usize);
                }
            }
            [left, right, ..] => {
                if let (Some(l), Some(r)) = unsafe { (samples(left), samples(right)) } {
                    context.writer.write_planar(l, r);
                }
            }
        }
    }));
    0
}

/// The buffers of an `AudioBufferList` (a variable-length C array).
unsafe fn buffers<'a>(list: NonNull<AudioBufferList>) -> &'a [AudioBuffer] {
    let list = list.as_ptr();
    let count = (*list).mNumberBuffers as usize;
    std::slice::from_raw_parts((*list).mBuffers.as_ptr(), count)
}

/// A buffer's f32 samples, if it has any and they are aligned.
unsafe fn samples<'a>(buffer: &AudioBuffer) -> Option<&'a [f32]> {
    let data = buffer.mData.cast::<f32>();
    if data.is_null() || (data as usize) % std::mem::align_of::<f32>() != 0 {
        return None;
    }
    let len = buffer.mDataByteSize as usize / std::mem::size_of::<f32>();
    Some(std::slice::from_raw_parts(data, len))
}

/// Stereo mix of every process except `own`, private to this process,
/// muted at its normal output while it is read.
fn tap_description(own: AudioObjectID, pid: u32, instance: u32) -> Retained<CATapDescription> {
    let excluded = NSArray::from_retained_slice(&[NSNumber::new_u32(own)]);
    unsafe {
        let description = CATapDescription::initStereoGlobalTapButExcludeProcesses(
            CATapDescription::alloc(),
            &excluded,
        );
        description.setName(&NSString::from_str(&format!(
            "TIWATON computer audio {pid}.{instance}"
        )));
        description.setPrivate(true);
        description.setMuteBehavior(CATapMuteBehavior::MutedWhenTapped);
        description
    }
}

/// This process's Core Audio process object (it exists once the process
/// uses audio; the engine starts its outputs first).
fn own_process_object() -> Result<AudioObjectID, String> {
    let pid = std::process::id() as i32;
    let address = global_address(kAudioHardwarePropertyTranslatePIDToProcessObject);
    let mut object: AudioObjectID = kAudioObjectUnknown;
    let mut size = std::mem::size_of::<AudioObjectID>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&address),
            std::mem::size_of::<i32>() as u32,
            (&pid as *const i32).cast(),
            NonNull::from(&mut size),
            NonNull::from(&mut object).cast(),
        )
    };
    if status != 0 || object == kAudioObjectUnknown {
        return Err(format!(
            "Computer audio could not leave this app out of the capture (Core Audio status {status}), so it was not started: it would feed the mix back into itself."
        ));
    }
    Ok(object)
}

/// The tap's stream format; only 32-bit float PCM is read.
fn tap_format(tap: AudioObjectID) -> Result<AudioStreamBasicDescription, String> {
    let address = global_address(kAudioTapPropertyFormat);
    let mut format = AudioStreamBasicDescription {
        mSampleRate: 0.0,
        mFormatID: 0,
        mFormatFlags: 0,
        mBytesPerPacket: 0,
        mFramesPerPacket: 0,
        mBytesPerFrame: 0,
        mChannelsPerFrame: 0,
        mBitsPerChannel: 0,
        mReserved: 0,
    };
    let mut size = std::mem::size_of::<AudioStreamBasicDescription>() as u32;
    check(
        unsafe {
            AudioObjectGetPropertyData(
                tap,
                NonNull::from(&address),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut format).cast(),
            )
        },
        "read the system audio format",
    )?;
    let float = format.mFormatID == kAudioFormatLinearPCM
        && format.mFormatFlags & kAudioFormatFlagIsFloat != 0
        && format.mBitsPerChannel == 32;
    if !float || format.mSampleRate < 1.0 {
        return Err(format!(
            "Computer audio arrived in a format this app cannot read ({} Hz, {} bits)",
            format.mSampleRate, format.mBitsPerChannel
        ));
    }
    Ok(format)
}

fn global_address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

fn check(status: i32, action: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("Computer audio could not {action} (Core Audio status {status})"))
    }
}

fn cf_key(key: &'static CStr) -> CFRetained<CFString> {
    CFString::from_str(key.to_str().unwrap_or_default())
}

fn cf_bool(value: bool) -> &'static CFBoolean {
    CFBoolean::new(value)
}

/// Store `key` → `value` in a CF dictionary created with the CFType
/// callbacks (which retain both).
fn set(dict: &CFMutableDictionary, key: &CFString, value: &CFType) {
    unsafe {
        CFMutableDictionary::set_value(
            Some(dict),
            (key as *const CFString).cast(),
            (value as *const CFType).cast(),
        );
    }
}

fn new_dictionary(capacity: isize) -> CFRetained<CFMutableDictionary> {
    unsafe {
        CFMutableDictionary::new(
            kCFAllocatorDefault,
            capacity,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    }
    .expect("CFMutableDictionary::new")
}

/// A private aggregate device that holds only the tap and starts it.
fn aggregate_properties(tap_uid: &NSString, uid: &str, name: &str) -> CFRetained<CFDictionary> {
    // SAFETY: NSString is toll-free bridged to CFString.
    let tap_uid: &CFString = unsafe { &*(tap_uid as *const NSString).cast::<CFString>() };
    let sub_tap = new_dictionary(2);
    set(&sub_tap, &cf_key(kAudioSubTapUIDKey), tap_uid);
    set(&sub_tap, &cf_key(kAudioSubTapDriftCompensationKey), cf_bool(true));
    let entries = [&*sub_tap as *const CFMutableDictionary as *const c_void];
    let taps = unsafe {
        CFArray::new(
            kCFAllocatorDefault,
            entries.as_ptr() as *mut *const c_void,
            entries.len() as isize,
            &kCFTypeArrayCallBacks,
        )
    }
    .expect("CFArray::new");

    let dict = new_dictionary(5);
    set(&dict, &cf_key(kAudioAggregateDeviceNameKey), &CFString::from_str(name));
    set(&dict, &cf_key(kAudioAggregateDeviceUIDKey), &CFString::from_str(uid));
    set(&dict, &cf_key(kAudioAggregateDeviceTapListKey), &taps);
    set(&dict, &cf_key(kAudioAggregateDeviceTapAutoStartKey), cf_bool(true));
    set(&dict, &cf_key(kAudioAggregateDeviceIsPrivateKey), cf_bool(true));
    unsafe { CFRetained::cast_unchecked::<CFDictionary>(dict) }
}

/// The "System Audio Recording" permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Granted,
    Denied,
    /// Not asked yet, or the check is unavailable.
    Unknown,
}

impl Permission {
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::Granted => "granted",
            Permission::Denied => "denied",
            Permission::Unknown => "unknown",
        }
    }
}

type PreflightFn = unsafe extern "C" fn(*const CFString, *const c_void) -> i32;

/// There is no public API to read this permission. The TCC framework's
/// `TCCAccessPreflight` (what Apple's own sample code and other capture
/// tools use) is loaded at run time; when it is missing the answer is
/// `Unknown` and capture is simply tried.
pub fn permission() -> Permission {
    static PREFLIGHT: OnceLock<Option<PreflightFn>> = OnceLock::new();
    let preflight = PREFLIGHT.get_or_init(|| unsafe {
        let library =
            libloading::Library::new("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC")
                .ok()?;
        let symbol = *library.get::<PreflightFn>(b"TCCAccessPreflight\0").ok()?;
        // Kept loaded for the life of the process.
        std::mem::forget(library);
        Some(symbol)
    });
    let Some(preflight) = preflight else {
        return Permission::Unknown;
    };
    let service = CFString::from_str("kTCCServiceAudioCapture");
    match unsafe { preflight(&*service, std::ptr::null()) } {
        0 => Permission::Granted,
        1 => Permission::Denied,
        _ => Permission::Unknown,
    }
}

#[cfg(test)]
mod probe {
    //! Manual check on a Mac: play music, then run
    //! `cargo test computer_audio_tap_probe -- --ignored --nocapture`.
    //! Needs the System Audio Recording permission for the app that runs
    //! the test (Terminal, an IDE…).
    use super::*;
    use crate::computer_audio::ComputerAudioStats;
    use ringbuf::traits::{Consumer, Observer, Split};
    use ringbuf::HeapRb;
    use std::sync::Arc;

    #[test]
    #[ignore = "needs a Mac, playing audio and the System Audio Recording permission"]
    fn computer_audio_tap_probe() {
        println!("permission: {}", permission().as_str());
        // Become an audio client first, as the engine does.
        let _ = cpal::traits::HostTrait::default_output_device(&cpal::default_host());
        let (producer, mut consumer) = HeapRb::<f32>::new(48_000 * 4).split();
        let stats = Arc::new(ComputerAudioStats::default());
        let tap = ProcessTap::start(|rate| CaptureWriter::new(producer, rate, 48_000, stats.clone()))
            .expect("tap");
        println!("tap rate: {} Hz", tap.capture_rate());
        std::thread::sleep(std::time::Duration::from_secs(2));
        let mut samples = vec![0.0f32; consumer.occupied_len()];
        let n = consumer.pop_slice(&mut samples);
        let peak = samples[..n].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        println!(
            "captured {} frames in 2 s, peak {peak:.3}, blocks {}",
            n / 2,
            stats.captured_blocks.load(Ordering::Relaxed)
        );
        drop(tap);
        assert!(n > 0, "the tap delivered nothing");
    }

    /// Peak of what the tap delivers over `seconds`.
    fn tap_peak(seconds: f32) -> (f32, usize) {
        let (producer, mut consumer) = HeapRb::<f32>::new(48_000 * 8).split();
        let stats = Arc::new(ComputerAudioStats::default());
        let tap = ProcessTap::start(|rate| CaptureWriter::new(producer, rate, 48_000, stats.clone()))
            .expect("tap");
        std::thread::sleep(std::time::Duration::from_secs_f32(seconds));
        drop(tap);
        let mut samples = vec![0.0f32; consumer.occupied_len()];
        let n = consumer.pop_slice(&mut samples);
        (samples[..n].iter().fold(0.0f32, |m, s| m.max(s.abs())), n / 2)
    }

    fn music(command: &str) {
        let _ = std::process::Command::new("osascript")
            .args(["-e", &format!("tell application \"Music\" to {command}")])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(700));
    }

    /// No feedback with the Monitor on a headset: this process plays a loud
    /// tone to `TIWATON_PROBE_OUTPUT` (default "Jabra Engage 50") the whole
    /// time. With Apple Music paused the tap must stay silent (this
    /// process is left out); with it playing the tap hears the music.
    /// `cargo test computer_audio_feedback_probe -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a Mac, the probe output device, Apple Music and the permission"]
    fn computer_audio_feedback_probe() {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let wanted = std::env::var("TIWATON_PROBE_OUTPUT").unwrap_or_else(|_| "Jabra Engage 50".into());
        let host = cpal::default_host();
        let device = host
            .output_devices()
            .unwrap()
            .find(|d| d.description().map(|x| x.name() == wanted).unwrap_or(false))
            .unwrap_or_else(|| panic!("output '{wanted}' not found"));
        let config = device.default_output_config().unwrap();
        let channels = config.channels() as usize;
        let rate = config.sample_rate() as f32;
        let mut phase = 0.0f32;
        let stream = device
            .build_output_stream(
                config.into(),
                move |data: &mut [f32], _| {
                    for frame in data.chunks_mut(channels) {
                        let v = (phase * std::f32::consts::TAU).sin() * 0.5; // −6 dBFS
                        phase = (phase + 1_000.0 / rate) % 1.0;
                        frame.fill(v);
                    }
                },
                |e| eprintln!("probe output error: {e}"),
                None,
            )
            .unwrap();
        stream.play().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));

        music("pause");
        let (silent_peak, silent_frames) = tap_peak(3.0);
        music("play");
        let (music_peak, music_frames) = tap_peak(3.0);
        drop(stream);
        println!("own 1 kHz tone at -6 dBFS on '{wanted}'");
        println!("music paused : peak {silent_peak:.5} over {silent_frames} frames");
        println!("music playing: peak {music_peak:.5} over {music_frames} frames");
        assert!(silent_frames > 48_000, "the tap delivered too little");
        assert!(silent_peak < 0.01, "the tap heard this process: feedback");
        assert!(music_peak > 0.01, "the tap did not hear Apple Music");
    }
}
