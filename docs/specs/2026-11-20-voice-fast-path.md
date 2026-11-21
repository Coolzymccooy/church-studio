# Voice fast path (live) vs full chain (stream)

**Status:** approved by the operator 20 Nov 2026 ("Yes, split it").

## Problem
With every voice feature on, the voice strip reached the room about 82 ms late. That was the 11 ms of audio buffers plus everything that waits ahead:
- RNNoise (20 ms)
- spectral noise reduction and dereverb (21 ms each)
- the 20 ms lookahead gate
- the 5 ms limiter

A voice that late in a PA or in a performer's ears sounds like an echo. The stream doesn't care, because AetherCast's lip-sync delay absorbs it.

## Contract
- **Full path:** `DspChain`, unchanged. Feeds **Stream**, and so the Stream recording and Stream NDI. LUFS, the spectrum source and "learn mic" stay here.
- **Live path:** `FastVoiceChain`. Feeds **Main, Monitor, Monitor 2, Monitor 3** and the automix detector. The order is:
  1. input gain
  2. HPF 80 Hz
  3. Hyper gate with a **3 ms** lookahead (same mode, threshold and learned fingerprint)
  4. warmth and clarity EQ
  5. de-esser
  6. compressor
  7. the full chain's auto-gain level (a 500 ms rider, shared so both paths sit at the same level)

  There's no limiter on this path; the bus limiters cover it. Its latency is 3 ms with the gate on and 0 with it off.
- **Strip:** both signals go through the same strip settings (trim, HPF, gate, EQ, compressor, fader, automix, pan, mute). Only the Stream send takes the full signal (`Strip::process_pre_split`, `Mixer::process_split`). Strips other than the voice strip are unchanged.
- **Reported latency:** `latency_ms` (header "lat") is the live path. The new `stream_latency_ms` is the Stream path.
- **Bypass:** both paths pass the raw signal.

## Trade-offs
- On the live path, gate onsets can open up to about 17 ms later than before, because the lookahead is shorter. Speech-only mode decides without RNNoise's voice activity detection.
- The room doesn't get RNNoise, noise reduction or dereverb. Those matter for the stream and recording, not for a room that already hears the voice acoustically.
