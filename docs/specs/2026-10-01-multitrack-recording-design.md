# Multitrack service recording

Roadmap phase 5-lite (issue #22). The desktop app records a whole service as one WAV file per mixer strip plus the Stream mix. Those files can then be remixed in any DAW (Audacity, Reaper, Logic), used for a "virtual soundcheck" on a quiet weekday, or edited into a sermon podcast. The browser build's existing single-file recorder (MediaRecorder) stays as it is. This feature is desktop-only.

## Decisions

1. **What is recorded.**
   - Every mixer strip is recorded **raw**: after input or NDI receive, but before the voice chain, EQ and fader. Raw tracks are what a remix or soundcheck needs.
   - The **Stream bus** is also recorded as a stereo track, exactly as it went to broadcast.
   - The **Main** bus can be added as a second stereo track. It is off by default.
   - Strips can be armed individually. By default every active strip is armed.
2. **Format.**
   - 32-bit float WAV (`WAVE_FORMAT_IEEE_FLOAT`) at the engine sample rate.
   - Strips are mono, buses are stereo.
   - Float means no clipping and no dither decisions.
   - The writer is a small hand-written RIFF writer (no new crate) with unit tests that read the header back.
3. **Crash safety.**
   - Each file's RIFF and data sizes are rewritten every ~5 s and on stop, so a power cut leaves files that play up to the last few seconds.
   - When a file nears 3.9 GB it rolls over to `… part 2.wav` (classic WAV is limited to 4 GB). That point is about 5.6 hours for a mono track at 48 kHz.
4. **Real-time path.**
   - The audio callback never touches the disk.
   - At engine start one SPSC ring (ringbuf 0.4) is preallocated, holding ~4 s of all record channels interleaved per frame.
   - While a recording is active (an `AtomicBool`), the callback pushes whole frames only; a frame that doesn't fit is dropped whole and counted.
   - There are no locks, allocation or logging in the callback.
   - A writer thread owns the consumer, de-interleaves, and writes each track through a `BufWriter`.
5. **Start and stop.**
   - Recording requires a running engine.
   - On start, the writer clears stale ring data before the flag is set.
   - On stop, the flag is cleared, the writer drains what remains, finalizes headers and writes `session.json`.
   - If the engine stops while recording, the recording is finalized first.
   - A disk write error stops the recording cleanly, keeps everything written so far, and is reported to the UI. The engine keeps running.
6. **Where files go.**
   - The default folder is `<Documents>/TIWATON Recordings/`. It can be changed with the folder picker (`tauri-plugin-dialog`, already a dependency) and is persisted in `recorder.json` in app data.
   - Each recording gets its own folder, `YYYY-MM-DD HHmm <optional title>/`.
   - The folder holds `01 <strip name>.wav` … `Stream Mix.wav` (and `Main Mix.wav`), plus `session.json`.
   - File names are sanitised (no path separators, reserved Windows names or trailing dots).
7. **`session.json`.** It holds:
   - the app version, sample rate and start time (local and UTC);
   - the tracks (file, name, kind strip/bus, channels, source hardware/NDI);
   - the duration, dropped-frame count and markers.

   It is enough for a later "import session" feature.
8. **Markers.** Each marker is `{ timeSeconds, label, source }`.
   - The operator can add one with an "Add marker" button.
   - A marker is added automatically whenever a mixer scene loads by any route: operator, Tiwaton Link (Lumina) or OBS link. This uses the single `load_scene_into_mixer` path, so a podcast editor can jump straight to "Sermon".
   - Markers are also written as a `cue ` chunk in `Stream Mix.wav` only if that is trivial. Otherwise they go in `session.json` only.
9. **Commands.**
   - `recorder_status`: state, elapsed time, folder, tracks, dropped frames, last error.
   - `recorder_get_config` / `recorder_set_config`: folder, include Main, armed strips.
   - `recorder_start { title? }` and `recorder_stop`.
   - `recorder_add_marker { label }`.
   - `recorder_open_folder`.
   - A `recorder-status` event fires about once a second while recording.
   - Tauri args are camelCase.
10. **UI.**
    - A **Record** section on the MIXER tab: a big record/stop button with an elapsed timer, armed-track chips, a folder picker, "Include Main mix", "Add marker", and a health line (dropped frames, last error).
    - After stop, a summary with **Open folder**.
    - A red **REC** pill with the timer appears in the status bar while recording.
    - Stopping asks for confirmation.
    - In the browser build the section says the feature is desktop-only.

## Out of scope (later)
Playback and editing timeline, punch-in, per-track sample-accurate alignment with OBS's video recording, compressed formats (FLAC/MP3), and import of a session back into the mixer for virtual soundcheck. `session.json` is designed so that import can be added later.

## Tests
- **Pure unit tests:**
  - WAV header bytes (float, mono/stereo, size patching, rollover naming);
  - file-name sanitising;
  - session.json shape;
  - frame interleave/de-interleave;
  - whole-frame drop counting;
  - marker timing (frames → seconds).
- **Integration test:** a writer thread fed from a ring on a temp dir. It writes, finalizes and reads back the sample values and sizes.
- **JS:** controller command names and arg shapes, timer formatting, panel state helpers.
