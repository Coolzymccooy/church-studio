# Live plugin hosting review spec

**Review state** · Proposed. Approve the architecture and release gates before implementation. The faceplate images show the proposed states.

## First release

Host locally installed VST3 audio effects on macOS and Windows and Audio Units on macOS. Offer two serial insert slots per chain, with at most four active chains across input strips and the Main and Stream masters. Input inserts run after internal strip processing and before the fader, automix gain, sends and DCA. Input detection keeps its existing tap. Bus inserts run before their delivery limiter. The recorder's raw tracks keep the same tap.

Negotiate mono on an input chain and stereo on a bus chain. Display `Unsupported layout` if a plugin cannot negotiate the required channels. The first release provides host-rendered parameters and presets. Instruments, MIDI, sidechain routing, vendor editor windows, bridged CPU architectures and network plugin installation stay outside the first release.

Validate the engine's active sample rate and test 44.1, 48, and 96 kHz for release. Cache discovery metadata by plugin format, class identifier, native architecture and bundle fingerprint. Scan the platform's registered Audio Units and standard VST3 locations in a disposable scanner process. A crashing or hanging scanner quarantines only that fingerprint. Scans are operator-initiated and never load code in the mixer process. [Steinberg plugin locations](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical%2BDocumentation/Locations%2BFormat/Plugin%2BLocations.html)

## Process boundary

Run one helper per active chain. The helper owns the instances, format-specific threading, state and parameter calls. The desktop process owns the mixer, devices, recorder and failover. On macOS, request system out-of-process AU instantiation where supported. Legacy AUv2 components loaded in-process by the AU API live in TIWATON's helper. The desktop host uses the same worker boundary in either case. [Apple AU hosting](https://developer.apple.com/documentation/audiotoolbox/incorporating-audio-effects-and-instruments) · [Apple AUv2 hosting APIs](https://developer.apple.com/documentation/audiotoolbox/hosting-audio-unit-extensions-using-the-auv2-api)

Use per-chain preallocated shared-memory SPSC audio rings. Fixed-size descriptors carry a generation, sequence and frame count. The control channel carries typed operations and bounded replies on a background thread. Duplicate or late blocks are discarded. The callback validates bounds and generation, checks output for finite samples, and never waits for a worker, allocates, locks or performs file I/O. The worker receives only its own chain's audio. The UI and remote mixer never open an IPC listener for plugin control.

Ship signed helpers with the minimum OS privileges needed by their format. On macOS, evaluate a sandboxed helper with read-only plugin discovery grants. The desktop executable retains library validation. Any necessary library-validation exception belongs to the narrowly scoped helper and requires a notarized distribution test. On Windows, evaluate a restricted token and a job object with process-tree, memory and CPU limits. The proposed isolation boundary is for failures. Claims about hostile plugins require a separately verified sandbox profile. Confirm scanner and worker compatibility with vendor licensing under the approved profile before release. [Apple Hardened Runtime](https://developer.apple.com/documentation/xcode/configuring-the-hardened-runtime) · [Apple library validation entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.cs.disable-library-validation)

## Timing and failover

Proposed worker quanta are 64, 128, or 256 frames. Default to 128 at 48 kHz after a device and load check. Adapt variable callback sizes using preallocated block adapters. Pin transport delay at two quanta per active chain for the lifetime of the graph. Add the sum of the plugins' reported processing delays. Report pipeline, plugin and total latency in samples and milliseconds. Confirm the transport budget with impulse tests before approving it for an actual service.

Read VST3 `IAudioProcessor::getLatencySamples()` after setup and handle `kLatencyChanged`. Read AU `AUAudioUnit.latency` or the bridged property in seconds and convert to the engine sample rate. Treat latency above the reserve of 2048 samples per chain, or an invalid value, as `Latency limit`. Keep the chain in aligned dry bypass until a new graph is explicitly applied. Apply topology and compensation changes at a block boundary after dry buffers have filled. Delay shorter parallel paths at each sum to match the longest path feeding that sum. Carry the existing gate, voice and limiter delays into path accounting. Intentional FX delays and reverb tails remain effects. [VST3 audio processor contract](https://steinbergmedia.github.io/vst3_doc/vstinterfaces/classSteinberg_1_1Vst_1_1IAudioProcessor.html) · [Apple AU latency](https://developer.apple.com/documentation/audiotoolbox/auaudiounit/latency)

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

## Licensing and release gates

Pin and audit the chosen Steinberg SDK release and its dependencies. The current upstream VST3 SDK is MIT licensed. Include its notice when redistributing covered code and review the separate trademark rules for any product marketing. Use Apple SDK frameworks for AU hosting. Third-party plugin binaries and vendor licenses are installed and authorized by the operator. Confirm the vendor terms for the named compatibility matrix, including Waves products, before advertising support. [Steinberg SDK license](https://github.com/steinbergmedia/vst3sdk/blob/master/LICENSE.txt) · [Steinberg usage guidance](https://github.com/steinbergmedia/vst3sdk#license--usage-guidelines)

After approval, implement tests for the generation protocol, bounds, two-quantum delay, compensation and sample alignment under variable callbacks. Use synthetic plugins that report latency, crash, hang, misreport buffer sizes and return non-finite output. Verify that bypass keeps the dry impulse aligned and that recording and unaffected channels continue. Measure callback headroom with four active chains on both supported operating systems and at each supported rate. Verify licensing helpers, discovery, suspension, restart, shutdown, notarization and Windows signing before a physical church rehearsal.

### Faceplate preview

The static previews illustrate slot states and the latency readout at desktop width. [Graphite](../screenshots/2026-11-20-plugin-hosting/graphite.png) · [Bone](../screenshots/2026-11-20-plugin-hosting/bone.png)
