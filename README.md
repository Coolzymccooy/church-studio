# TIWATON AI Studio

TIWATON AI Studio is a live voice-processing workstation for churches. It cleans up a
preacher/worship mic feed in real time and lets you export or route the result — a
"digital sound engineer" for services that don't have one.

Processing chain, front to back:

- **HyperGate** — a fast, FFT-based voice/noise gate (AudioWorklet in the web build,
  native Rust in the desktop build) that opens on speech and closes on room noise.
- **RNNoise neural denoise** — [xiph/rnnoise](https://github.com/xiph/rnnoise) running in
  an AudioWorklet via [`@sapphi-red/web-noise-suppressor`](https://github.com/sapphi-red/web-noise-suppressor)
  on the web build; a native implementation on desktop.
- **Dynamic de-esser** — sidechain sibilance detection ahead of compression.
- **Spectral de-reverb** — overlap-add minimum-statistics dereverberation.
- **4-band multiband compressor** (sub / low-mid / mid / air) plus a broadband compressor
  and a brick-wall limiter.
- **LUFS metering** (ITU-R BS.1770-4: momentary / short-term / integrated) for streaming
  loudness targets.
- **Sermon editor** — a waveform-based trim/edit view for recorded or uploaded sermon
  audio/video, used for the file-export workflow rather than the live path.

"Pastor Isolation" and similar presets are EQ + dynamics presets tuned for a speaking
voice, not source separation — they shape the existing signal, they don't isolate a
voice out of a mixed recording.

## Two builds

- **Web** — a browser build (this repo's Vite/React app), deployed at
  <https://church-studio-v56w.vercel.app/?app>. Runs entirely client-side using the Web
  Audio API and AudioWorklets; no audio leaves the browser.
- **Desktop (Tauri)** — `src-tauri/` wraps the same UI with a native Rust audio engine
  (`src-tauri/src/audio.rs`, `src-tauri/src/dsp/`) for lower latency and OS-level device
  routing.

## Getting audio into OBS today

There is **no direct OBS integration** yet — the app cannot hand OBS an audio stream by
itself. To use the processed output as an OBS source, route it through a virtual audio
cable:

1. Install a virtual audio device: [VB-CABLE](https://vb-audio.com/Cable/) on Windows, or
   [BlackHole](https://github.com/ExistentialAudio/BlackHole) on macOS.
2. In TIWATON AI Studio, set the monitor/output device to the virtual cable's input.
3. In OBS, add an "Audio Input Capture" source pointed at the virtual cable's output.

This works today but is a manual setup step for the operator, not a built-in feature.

## Development

```bash
npm install
npm run dev          # start the Vite dev server
npm run lint         # eslint
npm test             # node --test src/lib/*.test.js
npm run build        # production build (vite build)
npm run verify        # lint + test + build
```

Desktop-specific:

```bash
npm run tauri:dev     # run the Tauri desktop shell against the dev server
npm run tauri:build   # production desktop build
```

## Project layout

- `src/` — React UI, DSP chain wiring, and the pure/testable logic under `src/lib/`
  (`node --test src/lib/*.test.js`).
- `public/worklets/` — AudioWorklet processors used by the web build (HyperGate,
  de-esser, de-reverb, lookahead gate, LUFS meter). RNNoise's worklet ships from the
  `@sapphi-red/web-noise-suppressor` package instead of a hand-rolled one.
- `src-tauri/` — the Tauri desktop shell and native Rust audio engine
  (`src-tauri/src/audio.rs`, `src-tauri/src/dsp/`).
- `server/` — a small Express/ffmpeg helper service for file-based processing.
- `electron.legacy/` — an earlier Electron-based shell, kept for reference; not part of
  the active build.

## Roadmap

See [`docs/ROADMAP.md`](docs/ROADMAP.md) for planned work (not yet present on this
branch — it is being added separately).
