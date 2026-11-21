//! Hardware probes for USB interfaces (`docs/specs/2026-11-20-usb-interfaces.md`).
//! They need a real USB audio device, so they are ignored by default:
//!
//! `USB_PROBE_DEVICE="Jabra Engage 50" cargo test --lib usb_probe -- --ignored --nocapture --test-threads=1`
//!
//! - `usb_clock_probe`: the built-in microphone's clock drives the ring; the
//!   USB device plays it through a corrected feed for 20 s. Reports the
//!   drift the feed settled on and any underruns.
//! - `usb_hotplug_probe`: a public aggregate device wrapping the USB output
//!   appears, plays, is destroyed (as an unplug), and comes back under the
//!   same name; the stream is reopened by name and plays again.
use super::config::{input_config_at, output_config_at};
use super::device_names::find_device;
#[cfg(target_os = "macos")]
use super::device_names::device_names;
#[cfg(target_os = "macos")]
use super::health::{Reconnector, Step};
use super::health::{LinkState, StreamHealth};
use super::output_feed::{ClockMode, OutputFeed, OutputStats};
use super::streams::build_output;
use cpal::traits::{DeviceTrait, StreamTrait};
use parking_lot::Mutex;
use ringbuf::traits::{Observer, Producer, Split};
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const RATE: u32 = 48_000;
const BLOCK: u32 = 256;

fn usb_name() -> String {
    std::env::var("USB_PROBE_DEVICE").unwrap_or_else(|_| "Jabra Engage 50".to_string())
}

#[test]
#[ignore = "needs a USB audio device; run manually"]
fn usb_clock_probe() {
    let host = cpal::default_host();
    let name = usb_name();
    let (_, usb) = find_device(&host, false, &name).unwrap().expect("USB output not connected");
    let (mic_name, mic) = find_device(&host, true, "MacBook Pro Microphone")
        .unwrap()
        .expect("built-in microphone");
    let out_cfg = output_config_at(&usb, RATE).unwrap();
    let in_cfg = input_config_at(&mic, RATE).unwrap();
    println!("in: {mic_name} {in_cfg:?}\nout: {name} {out_cfg:?}");

    let (mut producer, consumer) = HeapRb::<f32>::new(RATE as usize * 2).split();
    let stats = Arc::new(OutputStats::default());
    let mut feed = OutputFeed::new(consumer, vec![0], ClockMode::Corrected, RATE, BLOCK as usize, stats.clone());
    feed.set_device_channels(out_cfg.channels() as usize);
    let feed = Arc::new(Mutex::new(feed));
    let health = Arc::new(StreamHealth::new(LinkState::Ok));

    // The mic callback is the clock: each input frame pushes one quiet
    // 440 Hz stereo frame, exactly as the engine's input callback would.
    let pushed = Arc::new(AtomicU64::new(0));
    let counter = pushed.clone();
    let mut phase = 0.0f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / RATE as f32;
    let in_channels = in_cfg.channels() as usize;
    let input = mic
        .build_input_stream(
            cpal::StreamConfig {
                channels: in_cfg.channels(),
                sample_rate: RATE,
                buffer_size: cpal::BufferSize::Fixed(BLOCK),
            },
            move |data: &[f32], _| {
                for _ in 0..data.len() / in_channels {
                    let s = phase.sin() * 0.05;
                    phase = (phase + step) % (2.0 * std::f32::consts::PI);
                    if producer.vacant_len() >= 2 {
                        let _ = producer.try_push(s);
                        let _ = producer.try_push(s);
                    }
                }
                counter.fetch_add((data.len() / in_channels) as u64, Ordering::Relaxed);
            },
            |e| eprintln!("input error: {e}"),
            None,
        )
        .unwrap();
    let output = build_output(&usb, &out_cfg, BLOCK, feed.clone(), health.clone(), &name).unwrap();
    output.play().unwrap();
    input.play().unwrap();

    let started = Instant::now();
    let mut settled_underruns = None;
    while started.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_secs(2));
        let secs = started.elapsed().as_secs();
        println!(
            "{secs:>2}s pushed {} frames, drift {} ppm, underruns {}",
            pushed.load(Ordering::Relaxed),
            stats.drift_ppm(),
            stats.underruns()
        );
        if secs >= 6 && settled_underruns.is_none() {
            settled_underruns = Some(stats.underruns());
        }
    }
    drop(input);
    drop(output);
    let late = stats.underruns() - settled_underruns.unwrap_or(0);
    println!("underruns after settling: {late}; final drift {} ppm", stats.drift_ppm());
    assert_eq!(late, 0, "the corrected feed ran dry after settling");
    assert!(stats.drift_ppm().abs() < 1_000, "drift pinned at the nudge limit");
    assert_eq!(health.state(), LinkState::Ok);
}

#[cfg(target_os = "macos")]
pub(super) mod aggregate {
    //! A public aggregate device wrapping one real device: created and
    //! destroyed like a USB plug.
    use objc2_core_audio::{
        kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
        kAudioAggregateDeviceSubDeviceListKey, kAudioAggregateDeviceUIDKey,
        kAudioHardwarePropertyTranslateUIDToDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioObjectUnknown,
        kAudioSubDeviceUIDKey, AudioHardwareCreateAggregateDevice,
        AudioHardwareDestroyAggregateDevice, AudioObjectGetPropertyData, AudioObjectID,
        AudioObjectPropertyAddress,
    };
    use objc2_core_foundation::{
        kCFAllocatorDefault, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks,
        kCFTypeDictionaryValueCallBacks, CFArray, CFBoolean, CFDictionary, CFMutableDictionary,
        CFRetained, CFString, CFType,
    };
    use std::ffi::{c_void, CStr};
    use std::ptr::NonNull;

    fn key(k: &'static CStr) -> CFRetained<CFString> {
        CFString::from_str(k.to_str().unwrap())
    }

    fn dict() -> CFRetained<CFMutableDictionary> {
        unsafe {
            CFMutableDictionary::new(
                kCFAllocatorDefault,
                4,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            )
        }
        .unwrap()
    }

    fn set(d: &CFMutableDictionary, k: &CFString, v: &CFType) {
        unsafe {
            CFMutableDictionary::set_value(Some(d), (k as *const CFString).cast(), (v as *const CFType).cast());
        }
    }

    /// Destroy a leftover aggregate with this UID (from an interrupted run).
    pub fn destroy_uid(uid: &str) -> bool {
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyTranslateUIDToDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let cf_uid = CFString::from_str(uid);
        let qualifier: *const CFString = &*cf_uid;
        let mut id: AudioObjectID = kAudioObjectUnknown;
        let mut size = std::mem::size_of::<AudioObjectID>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&address),
                std::mem::size_of::<*const CFString>() as u32,
                (&qualifier as *const *const CFString).cast(),
                NonNull::from(&mut size),
                NonNull::from(&mut id).cast(),
            )
        };
        if status == 0 && id != kAudioObjectUnknown {
            let destroyed = unsafe { AudioHardwareDestroyAggregateDevice(id) };
            println!("destroyed leftover {uid} ({id}): status {destroyed}");
            return destroyed == 0;
        }
        false
    }

    /// Every aggregate UID the probes create.
    pub const PROBE_UIDS: [&str; 4] = [
        "tiwaton.hotplug.probe",
        "tiwaton.engine.probe",
        "tiwaton.monitor.probe",
        "tiwaton.plugin.probe",
    ];

    /// Destroy any probe aggregate still present.
    pub fn cleanup() {
        for uid in PROBE_UIDS {
            destroy_uid(uid);
        }
    }

    /// Destroys its aggregate when dropped, a panic included.
    pub struct Aggregate(pub AudioObjectID);

    impl Drop for Aggregate {
        fn drop(&mut self) {
            unsafe { AudioHardwareDestroyAggregateDevice(self.0) };
        }
    }

    pub fn create(name: &str, uid: &str, sub_uids: &[String]) -> Aggregate {
        let _ = destroy_uid(uid);
        let subs: Vec<CFRetained<CFMutableDictionary>> = sub_uids
            .iter()
            .map(|sub_uid| {
                let sub = dict();
                set(&sub, &key(kAudioSubDeviceUIDKey), &CFString::from_str(sub_uid));
                sub
            })
            .collect();
        let entries: Vec<*const c_void> =
            subs.iter().map(|sub| &**sub as *const CFMutableDictionary as *const c_void).collect();
        let list = unsafe {
            CFArray::new(
                kCFAllocatorDefault,
                entries.as_ptr() as *mut *const c_void,
                entries.len() as isize,
                &kCFTypeArrayCallBacks,
            )
        }
        .unwrap();
        let d = dict();
        set(&d, &key(kAudioAggregateDeviceNameKey), &CFString::from_str(name));
        set(&d, &key(kAudioAggregateDeviceUIDKey), &CFString::from_str(uid));
        set(&d, &key(kAudioAggregateDeviceSubDeviceListKey), &list);
        set(&d, &key(kAudioAggregateDeviceIsPrivateKey), CFBoolean::new(false));
        let props = unsafe { CFRetained::cast_unchecked::<CFDictionary>(d) };
        let mut id: AudioObjectID = 0;
        let status = unsafe { AudioHardwareCreateAggregateDevice(&props, NonNull::from(&mut id)) };
        assert_eq!(status, 0, "create aggregate");
        Aggregate(id)
    }

}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs a USB audio device; run manually"]
fn usb_hotplug_probe() {
    const PROBE: &str = "TIWATON hot-plug probe";
    let host = cpal::default_host();
    let name = usb_name();
    let (_, usb) = find_device(&host, false, &name).unwrap().expect("USB output not connected");
    let sub_uid = usb.id().unwrap().id().to_string();

    let make = || aggregate::create(PROBE, "tiwaton.hotplug.probe", &[sub_uid.clone()]);
    let mut agg = make();
    std::thread::sleep(Duration::from_millis(800));
    assert!(device_names(&host, false).iter().any(|n| n == PROBE), "probe device not listed");

    // Play a feed on it, like an engine output.
    let (mut producer, consumer) = HeapRb::<f32>::new(RATE as usize).split();
    let stats = Arc::new(OutputStats::default());
    let feed = Arc::new(Mutex::new(OutputFeed::new(consumer, vec![0], ClockMode::Shared, RATE, BLOCK as usize, stats)));
    let health = Arc::new(StreamHealth::new(LinkState::Ok));
    let open = |health: &Arc<StreamHealth>| {
        let (_, device) = find_device(&host, false, PROBE).unwrap().expect("probe device");
        let cfg = output_config_at(&device, RATE).unwrap();
        {
            let mut f = feed.lock();
            f.set_device_channels(cfg.channels() as usize);
            f.restart();
        }
        let stream = build_output(&device, &cfg, BLOCK, feed.clone(), health.clone(), PROBE).unwrap();
        stream.play().unwrap();
        stream
    };
    let drained = |producer: &mut ringbuf::HeapProd<f32>| {
        // Fill the ring, wait, and report how much the device consumed.
        while producer.vacant_len() > 0 {
            let _ = producer.try_push(0.0);
        }
        std::thread::sleep(Duration::from_millis(500));
        producer.vacant_len()
    };
    let mut stream = Some(open(&health));
    let before = drained(&mut producer);
    println!("playing: consumed {before} samples in 0.5 s");
    assert!(before > 1_000, "the probe device is not playing");

    // Unplug.
    drop(agg);
    let deadline = Instant::now() + Duration::from_secs(3);
    while health.state() == LinkState::Ok && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let listed = device_names(&host, false).iter().any(|n| n == PROBE);
    println!("after unplug: state {}, listed {listed}, error {:?}", health.state().word(), health.error());
    assert!(!listed, "destroyed device still listed");
    let mut reconnector = Reconnector::default();
    if health.state() == LinkState::Ok {
        // No error callback: the watcher's listing check catches it.
        assert_eq!(reconnector.tick(health.state(), listed), Step::Wait);
        assert_eq!(reconnector.tick(health.state(), listed), Step::MarkLost);
        health.fail(LinkState::Lost, "Disconnected");
    }
    assert_eq!(health.state(), LinkState::Lost);
    drop(stream.take());

    // Plug back in: the watcher reopens after two listed ticks.
    agg = make();
    let mut step = Step::Wait;
    for _ in 0..5 {
        std::thread::sleep(Duration::from_secs(1));
        let listed = device_names(&host, false).iter().any(|n| n == PROBE);
        step = reconnector.tick(health.state(), listed);
        if step == Step::Reopen {
            break;
        }
    }
    assert_eq!(step, Step::Reopen, "never reopened");
    stream = Some(open(&health));
    health.set(LinkState::Ok);
    let after = drained(&mut producer);
    println!("reconnected: consumed {after} samples in 0.5 s");
    assert!(after > 1_000, "the reopened stream is not playing");
    drop(stream);
    drop(agg);
    aggregate::cleanup();
}

/// The whole engine and the device watcher on a mock app: input and monitor
/// on an aggregate wrapping the USB device (one clock), mon 2 on a device
/// that is not connected. The aggregate is destroyed and recreated; the
/// watcher marks both streams lost, then reopens them by name.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs a USB audio device; run manually"]
fn usb_engine_probe() {
    use super::device_watch::{tick, Watch};
    use super::{DeviceSelection, EngineState, RunningEngine};
    use crate::dsp::DspParams;
    use crate::mixer_control::MixerControl;
    use tauri::Manager;

    const PROBE: &str = "TIWATON engine probe";
    let host = cpal::default_host();
    // A USB headset's mic and speaker can be two Core Audio devices: wrap both.
    let mut subs = Vec::new();
    for input in [true, false] {
        let (_, device) = find_device(&host, input, &usb_name()).unwrap().expect("USB device not connected");
        let uid = device.id().unwrap().id().to_string();
        if !subs.contains(&uid) {
            subs.push(uid);
        }
    }
    let make = || aggregate::create(PROBE, "tiwaton.engine.probe", &subs);
    let mut agg = make();
    std::thread::sleep(Duration::from_millis(800));

    let app = tauri::test::mock_app();
    app.manage(std::sync::Mutex::new(None::<RunningEngine>) as EngineState);
    let params = Arc::new(DspParams::defaults());
    // Quiet: the headset is the only output, and its monitor is turned down.
    params.monitor_gain_db.store(-40.0, Ordering::Relaxed);
    let mixer = MixerControl::new();
    let engine = RunningEngine::spawn(
        app.handle().clone(),
        params,
        mixer.link(),
        DeviceSelection {
            input_id: Some(PROBE.to_string()),
            monitor_output_id: Some(PROBE.to_string()),
            broadcast_output_id: None,
            main_output_id: None,
            monitor2_output_id: Some("TIWATON missing probe".to_string()),
            monitor3_output_id: None,
            output_pairs: [0; crate::dsp::mixer::NUM_BUSES],
            input_range: None,
        },
        Default::default(),
        Default::default(),
        false,
    )
    .expect("engine start");
    let state = app.state::<EngineState>();
    *state.lock().unwrap() = Some(engine);
    let devices = || state.lock().unwrap().as_ref().unwrap().devices_json();
    let word = |role: &str| {
        devices()
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["role"] == role)
            .map(|d| d["state"].as_str().unwrap().to_string())
            .unwrap()
    };
    println!("started: {}", devices());
    assert_eq!(word("input"), "ok");
    assert_eq!(word("monitor"), "ok");
    assert_eq!(word("monitor2"), "waiting");
    assert_eq!(devices()[1]["clock"], "shared");

    let handle = app.handle().clone();
    let mut watch = Watch::default();
    let run_ticks = |n: usize, watch: &mut Watch| {
        for _ in 0..n {
            std::thread::sleep(Duration::from_secs(1));
            tick(&handle, &host, watch);
        }
    };
    run_ticks(2, &mut watch);

    // Unplug.
    drop(agg);
    run_ticks(2, &mut watch);
    println!("unplugged: {}", devices());
    assert_eq!(word("input"), "lost");
    assert_eq!(word("monitor"), "lost");

    // Plug back in.
    agg = make();
    run_ticks(4, &mut watch);
    println!("replugged: {}", devices());
    assert_eq!(word("input"), "ok");
    assert_eq!(word("monitor"), "ok");
    assert_eq!(word("monitor2"), "waiting");
    let callbacks = || state.lock().unwrap().as_ref().unwrap().callback_avg_ms();
    let before = callbacks();
    std::thread::sleep(Duration::from_secs(1));
    println!("callback avg {before:.3} -> {:.3} ms", callbacks());

    if let Some(engine) = state.lock().unwrap().take() {
        engine.stop();
    }
    drop(agg);
    aggregate::cleanup();
}

/// Mid-service: the input on the built-in mic, main on the USB device (its
/// own clock), the monitor on a probe device. A second device is plugged in,
/// then the monitor's device is unplugged and plugged back. Main must keep
/// playing with no underrun and the engine lock must never stall. Run it in
/// release (the debug DSP is slower than real time):
/// `cargo test --release --lib usb_midservice_probe -- --ignored --nocapture`
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs a USB audio device; run manually"]
fn usb_midservice_probe() {
    use super::device_watch::{tick, Watch};
    use super::{DeviceSelection, EngineState, RunningEngine};
    use crate::dsp::DspParams;
    use crate::mixer_control::MixerControl;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use tauri::Manager;

    const MON: &str = "TIWATON monitor probe";
    const NEW: &str = "TIWATON plug-in probe";
    let host = cpal::default_host();
    let usb = usb_name();
    let uid_of = |input: bool, name: &str| {
        let (_, device) = find_device(&host, input, name).unwrap().expect("device");
        device.id().unwrap().id().to_string()
    };
    let usb_out = uid_of(false, &usb);
    let speakers = uid_of(false, "MacBook Pro Speakers");
    let make_mon = || aggregate::create(MON, "tiwaton.monitor.probe", &[usb_out.clone()]);
    let mut mon = make_mon();
    std::thread::sleep(Duration::from_millis(800));

    let app = tauri::test::mock_app();
    app.manage(std::sync::Mutex::new(None::<RunningEngine>) as EngineState);
    let params = Arc::new(DspParams::defaults());
    params.monitor_gain_db.store(-40.0, Ordering::Relaxed);
    let mixer = MixerControl::new();
    let engine = RunningEngine::spawn(
        app.handle().clone(),
        params,
        mixer.link(),
        DeviceSelection {
            input_id: Some("MacBook Pro Microphone".to_string()),
            monitor_output_id: Some(MON.to_string()),
            broadcast_output_id: None,
            main_output_id: Some(usb.clone()),
            monitor2_output_id: None,
            monitor3_output_id: None,
            output_pairs: [0; crate::dsp::mixer::NUM_BUSES],
            input_range: None,
        },
        Default::default(),
        Default::default(),
        false,
    )
    .expect("engine start");
    let state = app.state::<EngineState>();
    *state.lock().unwrap() = Some(engine);
    let entry = |role: &str| {
        let devices = state.lock().unwrap().as_ref().unwrap().devices_json();
        devices.as_array().unwrap().iter().find(|d| d["role"] == role).cloned().unwrap()
    };

    // A thread that keeps taking the engine lock, like the UI's status poll.
    let stop = Arc::new(AtomicBool::new(false));
    let worst_us = Arc::new(AtomicU64::new(0));
    let poller = {
        let (stop, worst_us, handle) = (stop.clone(), worst_us.clone(), app.handle().clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let started = Instant::now();
                drop(handle.state::<EngineState>().lock().unwrap());
                worst_us.fetch_max(started.elapsed().as_micros() as u64, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(10));
            }
        })
    };

    let handle = app.handle().clone();
    let mut watch = Watch::default();
    let ticks = |n: usize, watch: &mut Watch| {
        for _ in 0..n {
            std::thread::sleep(Duration::from_secs(1));
            tick(&handle, &host, watch);
        }
    };
    ticks(4, &mut watch);
    let main_start = entry("main");
    println!("settled: main {main_start}\n         monitor {}", entry("monitor"));
    let underruns = |e: &serde_json::Value| e["underruns"].as_u64().unwrap();
    let base = underruns(&main_start);

    // 1. Another device is plugged in mid-service.
    let plugged = aggregate::create(NEW, "tiwaton.plugin.probe", &[speakers.clone()]);
    let seen = Instant::now();
    ticks(2, &mut watch);
    let listed = super::device_names::device_names(&host, false).iter().any(|n| n == NEW);
    println!("plug-in: listed {listed} within {:?}; main {}", seen.elapsed(), entry("main"));
    assert!(listed, "a plugged-in device must be listed");
    assert_eq!(entry("main")["state"], "ok");

    // 2. The monitor's device is pulled.
    drop(mon);
    ticks(3, &mut watch);
    println!("unplugged: monitor {}\n           main {}", entry("monitor"), entry("main"));
    assert_eq!(entry("monitor")["state"], "lost");
    assert_eq!(entry("main")["state"], "ok");

    // 3. It comes back.
    mon = make_mon();
    ticks(4, &mut watch);
    let main_end = entry("main");
    println!("replugged: monitor {}\n           main {main_end}", entry("monitor"));
    assert_eq!(entry("monitor")["state"], "ok");
    assert_eq!(main_end["state"], "ok");

    stop.store(true, Ordering::Relaxed);
    poller.join().unwrap();
    let worst = worst_us.load(Ordering::Relaxed);
    println!("main underruns {} -> {}; worst engine-lock wait {worst} us", base, underruns(&main_end));
    assert_eq!(underruns(&main_end), base, "main ran dry while other devices came and went");
    assert!(worst < 100_000, "the engine lock stalled for {worst} us");

    if let Some(engine) = state.lock().unwrap().take() {
        engine.stop();
    }
    drop(plugged);
    drop(mon);
    std::thread::sleep(Duration::from_millis(300));
    aggregate::cleanup();
}

/// Remove any probe device an interrupted run left behind.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "touches Core Audio devices; run manually"]
fn usb_probe_cleanup() {
    aggregate::cleanup();
}
