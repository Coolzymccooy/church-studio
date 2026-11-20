# Virtual soundcheck

## Session and routing

Open a TIWATON multitrack `session.json` while the audio engine runs. Version 1 maps recorded mono strips by source kind and strip name. The recorded index breaks ties only among matching names and kinds. The popover flags unmatched or unavailable tracks and explains each skipped mapping in an InfoTip. Bus tracks are excluded. The selection lists each mapped destination and its recorded name so the operator can confirm the patch. Each destination starts in Live and can switch to Playback independently. A strip without a mapped track stays Live.

Playback enters before the voice chain, strip processing, sends and fader. The input selector ramps over 10 ms. While transport is stopped or buffering, a strip in Playback receives silence. The rail and affected scribbles show amber while Playback is selected and green while audio is playing. To resume a mic, select Live. Recording and soundcheck playback are exclusive. The engine starts with an empty soundcheck and all strips Live.

## Transport

| Control | Contract |
| --- | --- |
| Load | Pick a session manifest. A successful load stops playback and resets the playhead and all input selectors. |
| Play | Start or resume from the playhead. At the end, start at frame zero. |
| Stop | Halt playback, retaining the position and strip selections. |
| Loop | Repeat the complete session. Default off. |
| Locate | Seek to any time between zero and the session duration, rounded to an engine sample. Releasing the timeline slider locates in one step. Markers provide locate targets. |
| Unload | Stop and clear the session. Switch the selected inputs back to Live with a ramp. |

Every track reads from the same frame counter. Rollover parts join at the next sample. Short tracks are padded with silence. Session and engine sample rates can differ. A shared rational phase maps each engine frame to the same source-frame pair for every track and linear interpolation preserves their relative timing. The transport position is reported on the engine timeline. Linear interpolation has no anti-alias filter for downsampling. The session duration comes from the longest mapped WAV track. Recorded whole-frame drops are shown in the session's info tip.

## Commands and events

`soundcheck_load(path)`, `soundcheck_unload`, `soundcheck_set_source(index, playback)`, `soundcheck_play`, `soundcheck_stop`, `soundcheck_set_loop(enabled)`, `soundcheck_locate(seconds)`, and `soundcheck_status`. Commands return errors without applying an invalid change. Status holds session title, rate, duration, markers, track mapping, input selection, playhead, loop, playing/buffering and the last error. While the desk is visible it polls status, including asynchronous transport completion and underruns, at 150 ms. The desk polls the small `soundcheck_signal` separately for the rail and strip lamps, including while the panel is closed. Soundcheck never persists across engine restarts or in scenes.

## Failure behaviour and real-time limits

Reject malformed or unsupported manifests, duplicate strip mappings, missing or incompatible audio parts, and paths leaving the chosen session folder. Active destinations outside the recording are unaffected. Fail a load with no mapped tracks. A failed load preserves the prior session. Validate WAV headers off the callback thread. If the disk cannot keep up, the selected playback channels fall silent together and the shared playhead holds until the next aligned frame. If reading fails, stop and report a concise error. The current session remains available to inspect or unload.

The callback only reads the bounded SPSC frame chunks, touches atomics and fills existing input buffers. The worker owns all files, read buffers and resampling. Prefetch holds 1.2 seconds of output at the engine rate. Track caches are sized from the source rate and have staggered read boundaries. The callback applies shared 5 ms playback fades for transport changes and underrun. Loop fades fit inside the session length, and the playhead holds while a locate or underrun fades the previous output to silence. Use one shared transport generation to reject stale audio after seek, stop, or session replacement. Operate on the callback's block boundaries. Verify alignment, loop wrap, locate while playing, partial and multirate files, underrun, and live source switching in tests.
