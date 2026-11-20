# Automixer and auto-ducking: design note

Status: approved and built, 2026-11-19. Engine only (`src-tauri/src/dsp/mixer/`, `mixer_control.rs`,
`mixer_meters.rs`, `mixer_model.rs`). The mixer UI belongs to the faceplate session; it gets the
keys and readouts below.

## What it does

1. **Automixer (Dugan-style gain sharing).** Strips with `automix` on form one group. The group
   shares a constant total gain: each strip's gain is its share of the group's power,
   `g_i = sqrt(w_i / Σ w_j)`. One talker → that mic sits near 0 dB and the idle mics drop by the
   level difference (a mic 15 dB quieter drops about 15 dB). Two equal talkers → −3 dB each; a
   fresh group nobody has spoken into yet → −10·log10(N) dB each, so the room sound stays
   constant (no NOM build-up). After someone has spoken, the last-mic hold below applies instead.
   A group of one strip is always at unity.
2. **Speech weighting.** `w_i = P_i × s_i`: `P_i` is the strip's smoothed power, `s_i` its speech
   factor. Each automix strip runs an RNNoise VAD side-chain (the same `nnnoiseless`
   network as the voice chain, analysis only: its audio output is discarded, so it adds **no
   audio latency**). `s_i` goes from 0.2 (no speech) to 1 (speech), held for 300 ms after speech
   so the gaps between words are not penalised. Music bleed and noise therefore need about 7 dB
   more level than a talker to take the gain. RNNoise is trained at 48 kHz, so at any other engine
   rate from 8 to 192 kHz (44.1 kHz interfaces, 24/16 kHz Bluetooth mics) the detector signal is
   resampled to 48 kHz by per-sample linear interpolation, for analysis only. Outside that range
   there is no VAD and `s_i = 1` (pure level-based Dugan).
3. **Last-mic hold.** The shares follow the mics only while some automix strip is 15 dB over its
   own noise floor (a 2-second rolling-minimum tracker, so steady hum or constant music bleed
   raises the floor and stops counting). Below that, in the fading tail of the last word and in
   room noise, the shares are frozen: the last talker's mic stays open and nothing drifts, so the
   room sound is steady between sentences.
4. **Auto-ducking.** Strips with `duck` on (music: the Computer L/R pair from PR 38, a playback
   channel, a band submix) dip by `duck_depth_db` (default 12 dB) while any automix strip is
   *speaking* (10 dB over its floor, and VAD speech when the VAD runs). Dip in about 75 ms, hold
   500 ms after the last speech, recover in about 1 s.

## Where it sits in the strip

```
trim/HPF ─┬─ gate → EQ → comp → [pre-fader] → fader × automix × duck → [post] → pan/sends → buses
          └─ detector (level, noise floor, VAD side-chain)
```

- Detection taps the signal after trim and HPF (on the voice strip that is after the voice chain),
  before the gate, so the gate does not fight the detector.
- The automix and duck gains apply **post-fader**: they reach Main, Stream and a post-fader Monitor
  send. The pre-fader Monitor send and solo/PFL are untouched, so stage mixes do not pump and the
  operator always hears the full mic on PFL. Faders keep their meaning (the mix balance).
- Muted strips and strips with the fader off leave the group (they cannot hold the gain). Their
  automix gain and held share are frozen while out, so muting or unmuting never swells their
  bleed; they rejoin where they left off.
- `automix` and `duck` are exclusive on one strip: setting one clears the other.

## Time constants (internal, not keys)

| | Value |
|---|---|
| Level envelope | 5 ms attack, 80 ms release (power) |
| Automix gain | 5 ms up, 150 ms down |
| Max automix attenuation | 40 dB |
| Shares follow the mics | some member ≥ floor + 15 dB |
| Speaking (ducking, `speech`) | floor + 10 dB and VAD speech; floor = rolling min over 2 s of 10 ms frames |
| VAD speech factor | 0.2..1, 300 ms hold |
| Ducking | 25 ms dip time constant, 500 ms hold, 400 ms recovery time constant |

## Real-time rules (ROADMAP decisions 2 and 3)

- Everything is sized in `Mixer::new` (control thread): detector state, one RNNoise state per strip
  (8–192 kHz engines), and a per-strip gain buffer of `max_block`. The callback never allocates or locks,
  with one caveat: nnnoiseless's FFT library (easyfft) sets up its planner per thread on the first
  frame. The mixer runs one silent VAD frame in the engine's first block, so that one-time set-up
  happens at engine start, not when the first talker starts. (The voice chain's denoiser had the
  same hidden first-frame set-up; the warm-up covers it too, since it runs on the same thread.)
- `Mixer::process` becomes two passes: pass 1 runs every strip up to the pre-fader point and its
  detector into preallocated buffers; the automixer then computes per-sample gains across the
  group; pass 2 applies fader × automix × duck and sums into the buses.
- All state advances per sample (VAD on fixed 480-sample frames, floor on fixed 10 ms frames,
  counted in samples), so any block size gives identical output. A test enforces it.
- CPU: the VAD runs only on strips with `automix` on. Measured on the M5 Pro (release,
  `automix_cpu` probe): about 0.5% of one core per automix mic.

## Contract additions

Strip keys (`mixer_set_strip_bool` / `mixer_set_strip_param`, saved in scenes, old scene files
load with the defaults):

| Key | Type | Range / default |
|---|---|---|
| `automix` | bool | false. Joins the automix group; clears `duck`. |
| `duck` | bool | false. Dips while an automix strip speaks; clears `automix`. |
| `duck_depth_db` | f32 | 0…40, 12 |

`mixer_state` strip fields: `automix`, `duck`, `duck_depth_db` (settings) plus `automix_gr_db`
and `duck_gr_db` (live attenuation, positive dB, 0 while stopped).

`mixer-meters` strip fields (held maximum since the last event, like `gr_db`):
`automix_gr_db`, `duck_gr_db` (positive dB) and `speech` (bool: this strip's detector heard
speech since the last event).

Once PR 38 lands, the Computer L/R strips (`computer:left` / `computer:right`) start with `duck` on
when their feed is first assigned; the operator can turn it off.

## Tests (Rust)

- Two talkers on two automix strips (syllable-modulated signals, bleed at −15 dB): the talking
  strip is within 1 dB of unity, the other at least 10 dB down; swap talkers and the gains swap.
- Nobody talking after a talker: gains stay within 0.5 dB over 3 s (last-mic hold).
- Music strip with `duck`: dips by `duck_depth_db` ±1 dB while speech is present, back within
  1 dB of 0 after the recovery time; does not dip on noise alone.
- Speech factor: with the VAD forced (test hook) to "no speech" on a louder music strip in the
  group, the quieter talker keeps the gain.
- One automix strip → unity; muted strip leaves the group; same output for block sizes 64, 480
  and 1024; no NaN on silence.
