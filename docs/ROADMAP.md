# TIWATON AI Studio — Roadmap

**Goal:** one app a church (and later any small studio) can run a whole service on:
clean voices, a real mixer, routing to the stream and the room, remote guests, and
recording plus editing afterwards. The benchmarks are Logic Pro (mixing and
editing), Audacity (file clean-up), SonoBus (low-latency network audio) and
Loopback (routing).

## Where we are (v1.4.3, September 2026)

| Area | State |
|---|---|
| Voice chain (gate, de-esser, de-reverb, compressor, limiter, LUFS) | Real, in both the browser (AudioWorklets) and the desktop app (Rust) |
| Neural denoise (RNNoise) | Wired but never ran: the WASM binary was never shipped |
| Inputs | One device, every channel averaged into one mono voice |
| Outputs | Monitor plus one "broadcast" device (a virtual cable picked in OBS by hand) |
| Mixer, buses, scenes | None |
| Network audio, NDI, app integration | None |
| Editor | Single file: cut, fade, trim silence, "Sermon Master" |

## Decisions

1. **The desktop app (Tauri + Rust) is the product engine.** A browser cannot make
   virtual devices, send NDI, open 16-channel interfaces reliably, or do
   low-latency network audio. The web build stays as the demo and sound-check tool.
2. **The audio callback never allocates, locks or blocks.** Everything the callback
   needs is sized up front. This is what keeps pro apps glitch-free.
3. **Block size must not change the sound.** Spectral stages (denoise, de-reverb)
   carry their state across callbacks, so any buffer size gives identical output.
   Tests enforce it.
4. **Our own code for anything we ship.** SonoBus is GPLv3, so we learn from it
   but don't copy it. NDI uses the official SDK under its licence terms (free, with
   attribution).
5. **One control link for all Tiwaton apps** ("Tiwaton Link"): a local WebSocket
   with JSON messages, plus OSC for hardware controllers. Lumina, Aethercast and
   Biblefuel talk to the studio through it.

## Phases

### Phase 0 — Honest foundation (in progress)
- Allocation-free, lock-free audio callback; ring-buffer history for noise capture.
- Streaming STFT for denoise and de-reverb (no clicks at block edges).
- Neural denoise that really runs: `nnnoiseless` (RNNoise in Rust) on desktop, a
  shipped RNNoise WASM in the browser.
- Export labels tell the truth (WebM vs MP4); a real README.
- `cargo test` in CI.

### Phase 1 — Mixer console
- Every input channel of an interface becomes its own channel strip (no mono fold).
- Strip: trim, polarity, high-pass, gate, 3-band EQ, compressor, pan, mute, solo,
  fader, and a send level to each bus.
- Buses: **Main** (room), **Stream** (livestream/record), **Monitor** (stage/headphones),
  each with its own limiter and meter. The voice-cleanup chain is a strip preset.
- Scenes (Worship, Sermon, Announcements) recall the whole console in one click.
- Auto-ducking: music channels dip when a voice channel is active.
- Mixer UI in the desktop app with meters per strip.

### Phase 2 — Routing and I/O
- NDI audio out (clean stream mix straight into vMix/OBS/Aethercast) and NDI in.
- OBS control over obs-websocket: start/stop stream, switch scene on cue,
  loudness warnings.
- A Tiwaton virtual device. On macOS this is an audio server plug-in; on Windows it
  needs a signed driver, so until then we document VB-CABLE and prefer NDI.

### Phase 3 — Tiwaton Link (app integration)
- Local WebSocket server in the desktop app: scene changes, meters, alerts
  ("Mic 2 silent", "clipping"), transport (record start/stop).
- **Lumina Presenter:** slide cues switch audio scenes; live captions from the
  clean voice bus appear on Lumina outputs; audio alerts show on the operator screen.
- **Aethercast:** takes the Stream bus over NDI; Stream Guard loudness feeds its dashboard.
- **Biblefuel Studio:** a finished sermon recording goes to Biblefuel for transcript,
  scripture detection and short-video cutting; Biblefuel instrumentals come back as
  walk-in music with auto-ducking.
- OSC and MIDI mapping for hardware faders (X-Touch, Stream Deck).

### Phase 4 — Network audio (SonoBus class)
- Peer-to-peer Opus over UDP with an adaptive jitter buffer, 10–40 ms target.
- Remote guest or musician joins from a link; each remote peer becomes a mixer strip.
- Campus-to-campus feed.

### Phase 5 — Record and edit (Audacity / Logic-lite)
- Multitrack recording of every strip to WAV, so a service can be remixed.
- Non-destructive multitrack editor: clips, fades, gain envelopes, markers.
- Offline jobs: transcription (Whisper), filler and silence removal, loudness
  targets for podcast and YouTube, voice/music separation (the Biblefuel laptop worker).
- Plugin hosting (CLAP/VST3) once the core is stable.

## Honest distance to the benchmarks

| Benchmark | What it would take |
|---|---|
| Loopback | Phase 1 plus Phase 2; the virtual driver on Windows is the hard part |
| SonoBus | Phase 4, a focused few weeks once the mixer exists |
| Audacity | Phase 5 editor plus offline jobs; the voice tools are already ahead for speech |
| Logic Pro | Years for full parity (MIDI, instruments, plugins). The realistic target is Logic's **mixer and live-recording** side for services, which Phases 1–5 cover |
