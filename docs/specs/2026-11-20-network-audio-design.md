# Network audio: remote people as mixer strips

Phase 4 of `docs/ROADMAP.md` ("SonoBus class"). The operator chose the first use
case on 20 Nov 2026: **receive remote musicians or a remote preacher, each as their
own mixer strip.** Sending the program to another campus and remote monitoring come
later.

SonoBus is GPLv3 (roadmap decision 4). We take its ideas — UDP, small packets, a
jitter buffer per peer, each peer as an input with its own level — and write our
own protocol and code. Nothing is copied.

## How it works

The remote person runs TIWATON AI Studio too (Mac or Windows) and turns on
**Send to network** with a bus (Main by default) and the church's address. The
church's Studio turns on **Receive from network**. Each sender that arrives takes
the next free **Remote** strip.

```
remote Studio                                   church Studio
bus → [callback → SPSC ring] → sender thread ── UDP ──→ receiver thread
                                                          │ order, fill gaps
                                                          ▼
                                         per-peer SPSC ring → callback (DriftReader)
                                                          → Remote n strip
```

## Decisions

1. **Our own protocol over UDP** (`std::net`, no platform code, so Windows and macOS
   behave the same). One datagram carries 5 ms of audio (240 frames at 48 kHz).
   The header holds: magic, version, kind (audio, ping, pong, bye), a random session
   id per sender, sequence number, channel count (1 or 2), codec, peer name (in
   pings), and an authentication tag.
2. **Slice 1 sends 16-bit PCM.** About 0.8 Mbit/s for mono, fine on a LAN or the
   tailnet, and no new C library. Opus comes in slice 2 for internet links; its crate
   choice (libopus needs CMake on Windows) is decided then.
3. **Packets are signed, not encrypted.** Each packet carries an HMAC-SHA256 tag
   (truncated to 16 bytes) keyed from the room passphrase (`hmac` + `sha2`, pure
   Rust). Packets without the right tag are dropped and counted. Audio is readable on
   the wire, so slice 1 is for trusted networks: the church LAN or the tailnet (which
   is already WireGuard-encrypted). Encryption and replay protection across sessions
   come with the internet slice. New Remote strips start with the fader down, like
   every new strip.
4. **Receive: one UDP socket and one thread, up to 4 peer slots** (1 to 4, set by
   the operator). The slots are reserved at engine start, after the NDI strips, so
   the strip count never changes while running (the same rule as NDI). A peer takes
   the first free slot; a slot frees after a `bye` or 3 s without packets.
5. **Jitter buffer, slice 1.** The receiver thread writes each peer's samples into
   its ring in sequence order. A lost packet becomes 5 ms of faded silence so timing
   holds; late and duplicate packets are dropped and counted. The audio callback
   reads the ring with the NDI `DriftReader` held at the peer's buffer target
   (default 20 ms, 5–200 ms), which also corrects clock drift by ±0.1 %. The
   adaptive target comes in slice 2; slice 1 measures and shows the jitter so the
   operator can set it.
6. **Latency is measured and shown.** The receiver pings each peer once a second and
   the sender answers. Shown latency = packet (5 ms) + half the round trip + current
   buffer fill + the engine's own latency. The two-machine test also measures the
   true audio round trip (see Test plan).
7. **Send** copies the chosen bus into a lock-free ring in the callback, as NDI out
   does. A sender thread frames 240 samples, signs and sends them. Mono by default
   ((L + R) / 2); stereo is a setting.
8. **Real-time rules** (roadmap decisions 2 and 3): the callback only pushes and pops
   rings and touches atomics. Sockets, signing, logging and allocation stay on the
   network threads. With the drift nudge idle, the strip's output is the same for any
   block size; a test enforces it. (The nudge itself samples ring fill once per block,
   as NDI in does, and is bounded to ±0.1 %.)
9. **Settings** live in `app_data_dir()/net_audio.json` and apply at the next engine
   start, like NDI. Atomic save; a corrupt file falls back to "off" with a warning.
10. **UI:** a **Network** tab in the desk output router (one line in
    `desk/OutputRouter.jsx`, agreed with the desk session first) opening
    `src/components/NetworkAudioPanel.jsx`. Receive: on/off, port, slots, buffer,
    passphrase. Send: on/off, address, bus, name. Per peer: name, latency, buffer,
    lost packets. Explanations sit behind info tips.

## Slices (one PR each)

1. **This slice:** PCM over UDP on trusted networks; receive as strips; send a bus;
   latency readout; `netaudio_probe` test tool; Mac ↔ Windows test over the tailnet.
2. Opus; adaptive jitter buffer; better loss concealment.
3. Internet: a small rendezvous server for NAT traversal (SonoBus's "connection
   server" idea, could run on the existing Coolify host), encryption, join by link.
4. Send a monitor mix back to each remote person (two-way); campus-to-campus feed.

## Test plan

- **Unit (no network):** header encode/decode and rejection of bad magic, version,
  length and tag; PCM16 round trip; sequence ordering, gap fill, late and duplicate
  drops; slot assignment and timeout; block-size invariance of the strip output;
  settings sanitising and JSON round trip.
- **Loopback integration:** sender and receiver threads on `127.0.0.1` deliver a
  tone into a slot ring; ping/pong yields a round trip.
- **Two machines:** `cargo run --example netaudio_probe` on the Mac and on
  ajibowo over the tailnet. One side sends a click train; the other receives it
  through the real jitter buffer and sends it straight back; the first side detects
  the returning clicks. Audio round trip / 2 is the measured one-way latency, set
  against the ping estimate. Then the Mac app receives the Windows probe as a Remote
  strip.
- `cargo test` and `npm test` on the Mac; `cargo test` and the build on ajibowo in a
  job worktree.
