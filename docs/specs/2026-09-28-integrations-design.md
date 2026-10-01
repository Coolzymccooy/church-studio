# Integrations: NDI, OBS, Lumina, Aethercast

Phases 2 and 3 of `docs/ROADMAP.md`. This doc decides how TIWATON AI Studio (the desktop app) connects to the operator's other apps and to OBS. The findings behind it come from surveys of `aether2` (AetherCast, `main` @ 9ae2868) and `lumina-presenter` (`origin/master`), made on 28 Sep 2026.

## What the other apps already do

| App | Relevant facts |
|---|---|
| **AetherCast** | Receives NDI video **and audio** through a Node grandiose sidecar. Discovered NDI sources appear in `NdiSourceList` for the operator to route, and NDI audio flows into its Web Audio mixer. It has its own obs-websocket-shaped control protocol over Socket.io on :3001 (`src/lib/controlProtocol.ts`). |
| **Lumina Presenter** | Sends and receives NDI (grandiose). It already **pushes live-service events** to a configurable webhook with the `lumina-aether` protocol v1: `POST` with headers `x-lumina-event/-workspace/-session/-token`, body `{source, protocol, protocolVersion, event, sentAt, sequence, workspaceId, sessionId, payload}`. Events include `lumina.item.started`, `lumina.slide.changed` (with a `liveContent` snapshot: `contentKind` lyrics / scripture / media / announcement / blackout / idle), `lumina.countdown.started/ended` and `lumina.service.mode.changed`. Settings live in `ConnectModal`'s `aether` panel. |
| **OBS Studio** | obs-websocket v5 on port 4455, with optional password. |

## Decisions

1. **Clean audio reaches AetherCast and Lumina over NDI, not a new protocol.** Both apps already list NDI sources and take their audio. The Studio publishes an audio-only NDI source per chosen bus: "TIWATON Studio (Stream)", and optionally Main or Monitor. AetherCast needs no code change: the source appears in its NDI list.
2. **NDI is loaded at runtime from the installed NDI Runtime**, using `libloading`. Functions are resolved by their exported names (`NDIlib_initialize`, `NDIlib_send_create`, …); there is no function-table struct whose order could break. Nothing is linked at build time, so CI and machines without NDI still build and run; the NDI features show "NDI Runtime not installed" with a link to ndi.video/tools.
   - Search order: `NDI_RUNTIME_DIR_V6`, then `NDI_RUNTIME_DIR_V5`, then the default install folders, then the app's own folder.
   - We don't bundle the DLL for now. The operator already has the runtime for Lumina and AetherCast.
3. **NDI licence terms in the UI:**
   - write "NDI®" at first use;
   - add a footnote: "NDI® is a registered trademark of Vizrt NDI AB";
   - link to https://ndi.video wherever NDI is chosen.
   - No product name may contain "NDI".
4. **NDI send runs on its own thread, never on the audio callback.** The callback copies the bus into a lock-free ring buffer. A sender thread converts it to planar float (FLTP) in 480-sample frames (10 ms at 48 kHz) and calls `NDIlib_send_send_audio_v3` with `clock_audio = true` and synthesized timecode.
5. **NDI receive becomes mixer strips.** A receiver thread captures planar float audio and writes it into a per-source ring buffer. The engine reads it as extra strips after the hardware channels.
   - NDI and the sound card run on different clocks, so drift is corrected by holding the ring buffer near its target fill. A gentle ±0.1 % resampling nudge does this without clicks.
   - Received sources are mixed like any strip: fader, EQ, sends, voice chain.
   - Maximum 4 NDI inputs.
6. **OBS is controlled with the `obws` crate (obs-websocket v5).** Features:
   - connect with host, port and password;
   - show stream and record state;
   - start and stop streaming and recording;
   - pick the OBS scene.
   - **Scene link** (optional): loading a Studio scene can switch OBS to a mapped scene, and an OBS scene change can load a mapped Studio scene.
   - Passwords are kept in the app config, never logged.
7. **Tiwaton Link: the Studio listens for Lumina's existing events.**
   - The desktop app runs a small HTTP server on **127.0.0.1:4460** (or next free port) that accepts Lumina's `lumina-aether` v1 webhook unchanged, at `POST /api/lumina/bridge`. Lumina therefore only needs to know a second URL.
   - A random per-install token is required as `x-lumina-token`. The Studio shows the URL and token for the operator to paste into Lumina.
   - `GET /api/status` returns the app name, version, whether the engine is running, and the bus loudness. It is for a later "Audio connected" badge in Lumina.
   - The server binds to loopback only; LAN access is a later, opt-in decision.
8. **Automation rules map Lumina events to audio scenes.** A rule is `{ when: {event, contentKind?, itemType?}, then: {loadScene} }`. Defaults (editable):
   - lyrics goes to "Worship";
   - scripture and announcement go to "Sermon";
   - countdown started goes to "Walk-in".
   - A rule only fires when that scene exists.
   - Rules debounce for 1.5 s, so fast slide changes don't flap the mix.
   - Every automatic change is shown in a log with an "undo" (reload the previous scene).
9. **Lumina-side change (in the Lumina repo):** a "TIWATON Studio" target in `ConnectModal`, shaped like the `aether` panel, with its own URL, token and status. The existing bridge dispatch fans out to both targets. This goes in a separate PR in `lumina-presenter`.

## Build order (one PR each)

1. **NDI out.** Runtime loader, audio sender, and "Send to NDI" per bus in the output router.
2. **OBS control.** `obws` client, OBS panel, scene link.
3. **Tiwaton Link.** Loopback server, Lumina bridge endpoint, automation rules, and the UI for URL, token and rules.
4. **Lumina TIWATON target** (in `lumina-presenter`).
5. **NDI in.** Receiver, drift correction, NDI strips in the mixer.

## Test strategy
There is no local Rust toolchain, so CI is the compiler.
- **Tests that need no NDI runtime, OBS or network:** pure units, including FLTP conversion and framing, drift controller maths, the rule engine, bridge-request validation (headers, token, body shape), scene-link mapping, and runtime-search-path logic.
- **Integration tests use the real server on an ephemeral port**, with no external apps.
- **The CI lockfile guard** uploads the resolved `Cargo.lock` whenever a PR adds a crate.
