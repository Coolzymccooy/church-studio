# Mixer FX returns

## Signal flow

FX A is a stereo room reverb and FX B is a stereo quarter-note delay. Each input strip (hardware, network, computer and NDI) has a send to both FX buses. The sends tap the post-fader, post-automix and post-duck signal after mute and pan. Changing the source fader or mute changes new signal entering the effects. Existing tails continue. PFL never feeds an effect.

The effects produce wet-only stereo signals. FX A pads the input by about 24 dB and uses a 3 kHz one-pole damper in each comb feedback loop. Higher frequencies decay faster. Two dedicated return strips route those signals to Main, Stream and the three monitor mixes, maintaining stereo separation. Main and Stream are post-fader. Mon 1 is pre-fader by default, with a post-fader switch, and Mon 2 and Mon 3 are pre-fader. Return mute affects every mix. Return PFL follows the existing Mon 1 rules. Returns cannot send to either effect, so a routing change cannot create a feedback loop. Returns remain the same two strips across input layout changes and have no input or recorder indices. They do not add latency to the dry paths.

## Controls

| Control | Range | Default |
| --- | --- | --- |
| Input `send_fx_a_db`, `send_fx_b_db` | −90 to +10 dB, −90 is off | Off |
| Return fader | −90 to +10 dB | 0 dB |
| Return Main and Stream sends | −90 to +10 dB | 0 dB |
| Return Mon 1, 2 and 3 sends | −90 to +10 dB | Off |
| Return mute, solo, Mon 1 post-fader | On or off | Off |
| FX A decay | 0.3 to 5 seconds (low-band RT60) | 1.8 seconds |
| FX B tempo | 40 to 240 BPM | 120 BPM |
| FX B feedback | 0 to 80 percent | 25 percent |

Send, return and mute changes use the desk's 10 ms ramps. Decay and feedback coefficients ramp. Tempo changes crossfade between fixed delay taps over 30 ms. If tempo changes again during a crossfade, the latest request takes effect after that crossfade finishes. Tap tempo uses the median of up to four recent intervals. An interval outside 250–1500 ms starts a new sequence. Two taps are needed to set a tempo, and manually setting tempo clears the tap sequence.

The desk exposes two FX send knobs per input and an FX key that opens the return bank. The return bank shows two stereo strips with sends, meters, fader, mute and solo. FX A shows decay, and FX B shows tempo, feedback and Tap.

## Control, events and persistence

Input sends use `mixer_set_strip_param`. Return settings use `mixer_set_fx_param(id, key, value)` and `mixer_set_fx_bool(id, key, value)`. The IDs are `fx_a` and `fx_b`. Return keys are `fader_db`, the five `send_*_db` output keys, `mute`, `solo` and `monitor_post_fader`. Effect keys are `decay_s` for A and `tempo_bpm` and `feedback_pct` for B. Tap is resolved by the desk to a tempo write. The shared bus setters also address the return fader and mute by FX ID for the remote tablet. `mixer_state.fxReturns` holds the two returns in A/B order. `mixer-meters.fxReturns` carries ordered stereo post-fader peaks. The existing scene load and state refresh paths carry the new settings.

Scenes store the input sends and both return and effect settings. Loading a scene created before FX resets the effect settings and turns off FX sends. Signal history and tap timestamps live in memory. A fresh engine starts with empty effect buffers and the current desk settings. A scene recall uses the same ramps as live adjustments.

## Failure and real-time behaviour

Invalid return IDs and keys are rejected. Finite out-of-range values are clamped. The audio thread reads control atomics once per block and uses preallocated buffers and delay lines. Effect state flushes values below its signal floor and discards non-finite samples. At 48 kHz, both effects and return routing together must average below 25 percent of a 256-frame callback budget in the host test. Measure the worst block separately from the average when evaluating actual device deadlines.

Verify separate and simultaneous sends, dry-path transparency, source-fader and mute behaviour, monitor routing and return PFL, tempo and feedback timing, decay and finite tails, denormal suppression, continuous parameter transitions, scene migration, and both desktop finishes. Room voicing and actual callback headroom need an interface and a venue.
