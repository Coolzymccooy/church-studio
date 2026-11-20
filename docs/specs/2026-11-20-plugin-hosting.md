# Live plugin hosting review spec

**Review state** · Proposed. Approve the architecture and release gates before implementation. The faceplate images show the proposed states.

## First release

Host locally installed VST3 audio effects on macOS and Windows and Audio Units on macOS. Offer two serial insert slots per chain, with at most four active chains across input strips and the Main and Stream masters. Input inserts run after internal strip processing and before the fader, automix gain, sends and DCA. Input detection keeps its existing tap. Bus inserts run before their delivery limiter. The recorder's raw tracks keep the same tap.

Negotiate mono on an input chain and stereo on a bus chain. Display `Unsupported layout` if a plugin cannot negotiate the required channels. V1 provides host-drawn parameter and preset controls only. This keeps third-party windows and their platform event loops out of the desktop app, at the cost of vendor-specific visualizations and custom controls. Instruments, MIDI, sidechain routing, bridged CPU architectures and network plugin installation stay outside the first release.

Validate the engine's active sample rate and test 44.1, 48, and 96 kHz for release. Cache discovery metadata by plugin format, class identifier, native architecture and bundle fingerprint. Scan the platform's registered Audio Units and standard VST3 locations in a disposable scanner process. Give each candidate 5 seconds to answer after its scanner starts. On crash or timeout, terminate that scanner and persist the candidate fingerprint, format, identifier, result and timestamp in `plugin-quarantine.json` in app data. The Plugins popover shows a Quarantine list with each reason in an InfoTip and a two-press Clear key per entry. Clearing permits a fresh manual scan. A new fingerprint can be scanned independently. Scans are operator-initiated and never load code in the mixer process. [Steinberg plugin locations](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical%2BDocumentation/Locations%2BFormat/Plugin%2BLocations.html)

## Process boundary

Use the Coupler `vst3` Rust bindings (candidate version 0.3.0, declared `MIT OR Apache-2.0` in its manifest), with the MIT option for this host. TIWATON owns the native module loader, host interfaces and lifecycle in a Rust helper. On macOS the AU adapter uses Apple's AudioToolbox and AVFAudio frameworks. Pin and audit the bindings and their transitive dependencies before shipping. [Coupler Cargo.toml](https://raw.githubusercontent.com/coupler-rs/vst3-rs/master/Cargo.toml)

Run one helper per active chain. The helper owns the instances, format-specific threading, state and parameter calls. The desktop process owns the mixer, devices, recorder and failover. On macOS, request system out-of-process AU instantiation where supported. Legacy AUv2 components loaded in-process by the AU API live in TIWATON's helper. The desktop host uses the same worker boundary in either case. [Apple AU hosting](https://developer.apple.com/documentation/audiotoolbox/incorporating-audio-effects-and-instruments) · [Apple AUv2 hosting APIs](https://developer.apple.com/documentation/audiotoolbox/hosting-audio-unit-extensions-using-the-auv2-api)

Use per-chain preallocated shared-memory SPSC audio rings. Fixed-size descriptors carry a generation, sequence and frame count. The control channel carries typed operations and bounded replies on a background thread. Duplicate or late blocks are discarded. The callback validates bounds and generation, checks output for finite samples, and never waits for a worker, allocates, locks or performs file I/O. The worker receives only its own chain's audio. The UI and remote mixer never open an IPC listener for plugin control.

Ship signed helpers with the minimum OS privileges needed by their format. On macOS, evaluate a sandboxed helper with read-only plugin discovery grants. The desktop executable retains library validation. Any necessary library-validation exception belongs to the narrowly scoped helper and requires a notarized distribution test. On Windows, evaluate a restricted token and a job object with process-tree, memory and CPU limits. The proposed isolation boundary is for failures. Claims about hostile plugins require a separately verified sandbox profile. Confirm scanner and worker compatibility with vendor licensing under the approved profile before release. [Apple Hardened Runtime](https://developer.apple.com/documentation/xcode/configuring-the-hardened-runtime) · [Apple library validation entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.cs.disable-library-validation)

## Timing and failover

Proposed worker quanta are 64, 128, or 256 frames. Default to 128 at 48 kHz after a device and load check. Adapt variable callback sizes using preallocated block adapters. Pin transport delay at two quanta per active chain for the lifetime of the graph. Add the sum of the plugins' reported processing delays. Report pipeline, plugin and total latency in samples and milliseconds. Confirm the transport budget with impulse tests before approving it for an actual service.

The live ceiling is **10 ms of additional insert latency per complete input-to-output route**, counting transport and serial plugin processing across both strip and bus chains. Target 5–8 ms. Calculate the ceiling in samples at the active rate. The existing voice, gate and limiter delays are reported separately. Reject a quantum, chain or combination that exceeds the limit with `Latency limit`, leaving the dry route active.

Read VST3 `IAudioProcessor::getLatencySamples()` after setup and handle `kLatencyChanged`. Read AU `AUAudioUnit.latency` or the bridged property in seconds and convert to engine samples. Compensate parallel paths at the Main and Stream sums. Mon 1, Mon 2 and Mon 3 use their source path's own latency with zero additional parallel-path compensation, so a slow input cannot delay other musicians' feeds. Intentional FX delays and reverb tails remain effects. [VST3 audio processor contract](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IAudioProcessor.html) · [Apple AU latency](https://developer.apple.com/documentation/audiotoolbox/auaudiounit/latency)

Each chain reports a rolling p99 processing load as a percentage of one worker quantum, shown in the Plugins popover. Admission measures a candidate with restored state and representative audio over at least 256 quanta. Refuse it when p99 exceeds 40% of a quantum, when any quantum misses its deadline, or when warm-up of the combined graph puts the engine callback p99 above 70% of its device period. Budget both workers during a live replacement. Record p99, worst block and missed deadlines in release tests.

Every affected path keeps an aligned dry lane inside the desktop mixer. Any missing, stale, malformed or non-finite worker result selects that dry lane for the current quantum. Fade from the last good output to dry over up to 64 samples, with no added wait. A crashed worker, three consecutive missed deadlines, or five misses in a second marks the chain `Bypassed` and stops the worker off the callback. An explicit Retry starts a fresh worker and crossfades in only after two consecutive valid blocks. Keep the aligned dry lane active even if initial state restore or authorization fails. Recorder and unaffected channels continue throughout.

## Controls, state and persistence

| Control | Contract |
| --- | --- |
| Slot | 1–2 per chain. Empty by default. Choose from validated scan results. |
| Bypass | On or off. New instances stay bypassed until allocation, latency and output validation succeed and the operator enables them. |
| Parameter | Normalized 0 to 1 with the plugin's range, display units, quantization, default and bounded gesture events. |
| Retry | Available on a fault. One operator request per worker generation. |
| Remove or replace | Two-press key during live mixing. First stage the dry graph and apply by the same block-boundary transition. |

The Plugins rail opens a popover for the selected mixer strip or bus. Each slot shows its name, state, bypass and latency. The parameter page uses existing knobs and keys. Error details and latency breakdown belong in InfoTips.

Proposed commands are `plugin_scan`, `plugin_catalog`, `plugin_prepare_insert`, `plugin_set_bypass`, `plugin_set_parameter`, `plugin_remove`, and `plugin_retry`. Mutations carry the chain and slot identity and expected generation. Snapshots carry effective graph generation, sample rate, slot identity, state, bypass, host and plugin latency, and fault code. `plugin-rack-changed` emits on topology, bypass, fault or availability changes. Read parameter and meter feedback at a bounded UI rate. Return explicit errors for stale generations, invalid values, unsupported formats, and failed setup.

Scenes store ordered insert IDs, bypass state and bounded opaque plugin presets in application-owned storage. Maximum preset size is 1 MiB per insert. A scene can resolve only a scanned identity, never a supplied executable path. Old scenes have empty slots. A missing plugin, failed license check or incompatible version leaves that insert dry and keeps the saved preset for recovery. Save scenes using the existing replacement workflow. Vendor state and diagnostics stay on the desktop.

Arm a fixed latency reserve for each compensated sum at graph start. During live recall, load presets, negotiate channels and verify latency and CPU on a separate worker while the current graph keeps playing. Give each preparation up to 15 seconds and show `Preparing` while it runs. Once two valid blocks arrive, pad the new wet and dry lanes to the existing reserve and crossfade between lanes over 10 ms at a block boundary, using identical input sample positions. No live change resets a read cursor or changes a compensation delay. If preparation fails, exceeds a deadline or needs a new reserve, retain the current insert and mark the requested plugin change `Pending` for an explicit off-air reconfiguration. Ordinary scene mixer controls apply at their existing recall boundary.

## Licensing and release gates

Pin and audit the chosen Steinberg SDK release and its dependencies. The current upstream VST3 SDK is MIT licensed. Include its notice when redistributing covered code and review the separate trademark rules for any product marketing. Use Apple SDK frameworks for AU hosting. The operator installs and activates third-party effects through their vendor tools, including PACE License Support and iLok License Manager where required, or Waves Central for Waves licenses. Verify lookup from the signed helper for each supported activation method before advertising it. Exercise local machine and USB activation offline. Where the publisher requires a supported cloud mode, test connectivity loss and leave the insert in aligned dry bypass on failure. Only the licensing access confirmed necessary by the vendor is allowed in the helper profile. Activation prompts and recovery run off-air. A failed check shows `License needed` with the vendor in an InfoTip. [iLok activation locations](https://help.ilok.com/ilm_overview.html) · [Waves activation](https://www.waves.com/support/activate-licenses)

 Confirm the vendor terms for the named compatibility matrix, including Waves products, before advertising support. [Steinberg SDK license](https://github.com/steinbergmedia/vst3sdk/blob/master/LICENSE.txt) · [Steinberg usage guidance](https://github.com/steinbergmedia/vst3sdk#license--usage-guidelines)

After approval, implement tests for the generation protocol, bounds, two-quantum delay, compensation and sample alignment under variable callbacks. Use synthetic plugins that report latency, crash, hang, misreport buffer sizes and return non-finite output. Verify that bypass keeps the dry impulse aligned and that recording and unaffected channels continue. Measure callback headroom with four active chains on both supported operating systems and at each supported rate. Verify licensing helpers, discovery, suspension, restart, shutdown, notarization and Windows signing before a physical church rehearsal.

### Faceplate preview

The static previews illustrate slot states and the latency readout at desktop width. [Graphite](../screenshots/2026-11-20-plugin-hosting/graphite.png) · [Bone](../screenshots/2026-11-20-plugin-hosting/bone.png)
