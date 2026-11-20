# Live mixer: engine ↔ UI contract

Phase 1 of `docs/ROADMAP.md`: this wires the mixer DSP core (`src-tauri/src/dsp/mixer/`) to real I/O and a mixer screen.

## Engine behaviour

- **Every input channel of the selected device becomes its own strip.** There is no mono fold. The number of strips equals the device's input channel count, capped at 32.
- **The voice chain is an insert on at most one strip.** The existing voice-cleanup chain (`DspChain`) runs on strip 0 by default. Its position is the strip-level bool `voice_chain`. Turning it on for one strip turns it off for the others. Turning it off on the strip that holds it is allowed: then no strip runs the voice chain and every strip reports `voice_chain: false`.
- **Output mapping:**

  | Bus | Output device | Notes |
  |---|---|---|
  | Stream | `broadcastOutputDevice` | to OBS/NDI/record; same as today's "broadcast" |
  | Monitor | `monitorOutputDevice` | operator headphones/stage; `monitor_gain_db` still applies (Monitor only) |
  | Main | `mainOutputId` | optional, new; the PA feed. Not opened when absent |

  A device is opened once, for the first bus that claims it, in the order Monitor → Stream → Main: a broadcast device equal to the monitor is skipped, and a main device equal to the monitor or broadcast device is skipped.

- **Backwards compatibility.** With one input channel and the default state, the Stream and Monitor outputs sound like today's app. Default state:
  - strip 0 has `voice_chain` on, fader 0 dB and centre pan;
  - strips 1 and up have their fader off (−90 dB, i.e. −inf), so a 2-channel mic still sounds like one channel until the operator raises a fader;
  - sends: Main 0 dB, Stream 0 dB, Monitor 0 dB pre-fader;
  - every bus fader is at 0 dB.
- **Stereo out.** Buses are stereo. A 1-channel output device gets (L+R)/2; outputs with more channels get L/R on channels 1/2 and silence on the rest.
- **Pan law.** Constant power, normalised to 0 dB at centre (+3 dB on one side at hard pan), so a centred mono strip arrives at the level of the old mono path.
- **Latency: stages that are off add none.** Strip gates use the same switched, zero-latency-when-off pattern as the voice chain, and default to off. With the defaults the mixer adds only the bus limiter's 5 ms. The reported latency sums what is active: the slowest strip (voice chain on its strip, plus 20 ms gate lookahead on any strip whose gate is on; strips run in parallel, so the max is taken) plus the bus limiter.

## Tauri commands (snake_case names, camelCase args as Tauri maps them)

| Command | Args | Returns |
|---|---|---|
| `start_audio_engine` | `inputDevice`, `monitorOutputDevice`, `broadcastOutputDevice`, `mainOutputId` (all optional strings; null = default device for input/monitor, not opened for broadcast/main) | existing JSON plus `inputChannels` and `main_output_name` |
| `mixer_state` | none | `MixerState` |
| `mixer_set_strip_param` | `index: u32, key: String, value: f32` | `()` or error string |
| `mixer_set_strip_bool` | `index: u32, key: String, value: bool` | `()` or error |
| `mixer_rename_strip` | `index: u32, name: String` (max 24 chars, trimmed) | `()` or error |
| `mixer_set_bus_param` | `bus: String ("main"\|"stream"\|"monitor"), key: String, value: f32` | `()` or error |
| `mixer_set_bus_bool` | `bus, key, value: bool` | `()` or error |
| `mixer_list_scenes` | none | `Vec<String>` |
| `mixer_save_scene` | `name: String` | `()`; stored as JSON in `app_data_dir()/scenes/<slug>.json` |
| `mixer_load_scene` | `name: String` | `MixerState` |
| `mixer_delete_scene` | `name: String` | `()` |

Rules:
- An unknown key, a bad index or a bad bus is an `Err(String)`, never a panic. Values are clamped to the ranges below.
- Scene names are 1–40 characters of letters, digits, space, `-` and `_`. The slug is lowercase, with spaces replaced by `-`. Anything else is rejected, so no path traversal is possible.
- The mixer state works while the engine is stopped (params live in shared state), so the UI can prepare a mix before going live.

### Strip keys

| Key | Type | Range / default |
|---|---|---|
| `trim_db` | f32 | −20…+40, 0 |
| `polarity` | bool | false |
| `hpf_enabled` | bool | true |
| `hpf_hz` | f32 | 20…400, 80 |
| `gate_enabled` | bool | false |
| `gate_threshold_db` | f32 | −80…0, −45 |
| `eq_low_db`, `eq_mid_db`, `eq_high_db` | f32 | −15…+15, 0 |
| `comp_enabled` | bool | false |
| `comp_threshold_db` | f32 | −40…0, −18 |
| `comp_ratio` | f32 | 1…20, 3 |
| `pan` | f32 | −1…+1, 0 |
| `mute` | bool | false |
| `solo` | bool | false |
| `fader_db` | f32 | −90…+10; 0 on strip 0, −90 (off) on strips 1+ (≤ −90 means off) |
| `send_main_db`, `send_stream_db`, `send_monitor_db` | f32 | −90…+10, 0 |
| `monitor_post_fader` | bool | false |
| `voice_chain` | bool | true on strip 0 only; at most one strip (none after turning it off on the holder) |
| `automix` | bool | false. Joins the automix (gain-sharing) group. Turning it on clears `duck` |
| `duck` | bool | false. Music strip: dips while an automix strip speaks. Turning it on clears `automix`. Computer-audio strips (`computer:left` / `computer:right`) start with it on |
| `duck_depth_db` | f32 | 0…40, 12. How far a `duck` strip dips |

Automix and ducking (design: `2026-11-19-automix-ducking-design.md`) apply post-fader: they reach Main, Stream and a post-fader Monitor send, never the pre-fader Monitor send or PFL. They add no latency. Muted strips and strips with the fader off leave the automix group.

### Bus keys
`fader_db` f32 (−90…+10, 0), `mute` bool, `limiter_ceiling_db` f32 (−12…0, −1), `leveler_enabled` bool (false), `leveler_target_lufs` f32 (−30…−10, −14).

**Loudness leveler** (`leveler_enabled`, `leveler_target_lufs`; meant for the Stream bus, works on any bus). A slow leveler between the bus fader and the bus limiter steers the bus toward the target: BS.1770 K-weighted, both channels summed, measured over the last 3 s of programme. A 100 ms block counts as programme only when the 400 ms before it and the 400 ms after it are both no more than 20 LU under the target, so silence and a quiet room freeze the gain instead of pumping it up. The gain is bounded to ±6 dB and moves at most 3 dB/s; switching the leveler off ramps it back to 0 dB at that rate, then the bus passes audio unchanged. It adds no latency, and the bus limiter still holds the ceiling. The UI's "Stream safe" switch and loudness target drive these keys on the Stream bus. They are delivery settings, not mix settings: scenes neither save nor recall them, and they keep their values until the app quits.

## MixerState (JSON)

```json
{
  "inputChannels": 2,
  "computerStrips": 0,
  "ndiStrips": 0,
  "running": true,
  "strips": [{ "index": 0, "name": "Ch 1", "trim_db": 0, "polarity": false, "hpf_enabled": true, "hpf_hz": 80,
    "gate_enabled": false, "gate_threshold_db": -45, "eq_low_db": 0, "eq_mid_db": 0, "eq_high_db": 0,
    "comp_enabled": false, "comp_threshold_db": -18, "comp_ratio": 3, "pan": 0, "mute": false, "solo": false,
    "fader_db": 0, "send_main_db": 0, "send_stream_db": 0, "send_monitor_db": 0,
    "monitor_post_fader": false, "voice_chain": true,
    "automix": false, "duck": false, "duck_depth_db": 12, "automix_gr_db": 0, "duck_gr_db": 0 }],
  "buses": [{ "id": "main", "fader_db": 0, "mute": false, "limiter_ceiling_db": -1,
              "leveler_enabled": false, "leveler_target_lufs": -14 },
            { "id": "stream", "fader_db": 0, "mute": false, "limiter_ceiling_db": -1,
              "leveler_enabled": true, "leveler_target_lufs": -14 },
            { "id": "monitor", "fader_db": 0, "mute": false, "limiter_ceiling_db": -1,
              "leveler_enabled": false, "leveler_target_lufs": -14 }],
  "scenes": ["Sermon", "Worship"]
}
```
`inputChannels` is the running engine's input channel count, or 1 when stopped; `strips` lists one entry per input channel (capped at 32). `ndiStrips` is the number of received NDI® sources (0–4, see decision 5 of `2026-09-28-integrations-design.md`); they are extra strips after the hardware ones, named after the source, and the total never exceeds 32. `computerStrips` is 0, or 2 when the computer audio input runs (what this computer plays, as "Computer L" and "Computer R"); that pair sits right after the hardware strips and before the NDI strips, which stay the last `ndiStrips` entries. A new computer pair starts panned hard left and right, trimmed −3 dB to cancel the hard pan's +3 dB, with the Monitor send after the fader, and with HPF, gate and the voice chain off. They start with the fader off, like every strip after the first. Strip settings are kept by index. At each engine start a source that moved to another index (the computer pair or an NDI input, when the input device gains or loses channels) takes its settings, name and voice chain with it; a strip whose feed is new (hardware ↔ NDI, or another NDI source) is reset to the new-strip defaults and its default name, and loses the voice chain (none is set). A source name only replaces a default name or the previous automatic name, never the operator's rename. `automix_gr_db` and `duck_gr_db` are live readouts (positive dB of attenuation, 0 while the engine is stopped), not settings. The top-level fields are camelCase (`inputChannels`, `running`). Strip and bus fields use the snake_case key names, so the UI sends back exactly the key it reads.

## Event `mixer-meters` (~20 Hz while running)

```json
{ "strips": [{ "pre_db": -30.1, "post_db": -33.0, "gate_open": true, "gr_db": 2.5,
               "automix_gr_db": 9.0, "duck_gr_db": 0, "speech": false }],
  "buses": [{ "id": "main", "peak_l_db": -12, "peak_r_db": -12, "rms_l_db": -20, "rms_r_db": -20,
              "leveler_db": 0 }] }
```
dB values are clamped to ≥ −96. `leveler_db` is the gain the bus loudness leveler applies now (signed dB, −6…+6; 0 when off). `gr_db` (compressor), `automix_gr_db` and `duck_gr_db` are positive amounts of attenuation, held at their maximum since the last event. `speech` is true if the strip's automix detector heard speech since the last event (automix strips only).
