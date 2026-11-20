# Auto-mix and ducking: operator guide

**Auto-mix** keeps several mics balanced on its own: the mic someone is talking into stays up, the
others come down, and the room never gets the build-up of many open mics.
**Duck** dips music (computer audio, a playback channel) while someone is talking, then brings it
back.

## 1. Before the service

1. **One input device for all mics.** The engine opens one input device; each of its channels is a
   strip. Use the church's audio interface. On a Mac with separate USB mics, combine them first:
   Audio MIDI Setup (Applications → Utilities) → **+** → **Create Aggregate Device**, tick the mics,
   tick **Drift Correction** on every one except the clock source, and pick the aggregate as Input
   in **setup**.
2. **Avoid Bluetooth mics.** A Bluetooth headset mic (AirPods and similar) drops the engine to 24 or
   16 kHz and can cut its own headphone output. Use Bluetooth only for the monitor, and pick a wired
   or USB input instead of "System default".
3. **Start the engine.** **start** in the header. (**stop** needs two presses within 3 s.) Device
   changes made in setup take effect at the next start.
4. **Check the strips.** One per mic, then **Computer L / Computer R** if computer audio is on. New
   strips start with the fader down: raise them.
5. **Match the mics.** With someone talking normally into each mic in turn, set **trim** so the
   strips' meters peak about the same. Auto-mix assumes matched mics.

## 2. Set each strip's role

| Strip | `auto` | `duck` | Notes |
|---|---|---|---|
| Every speech mic (pulpit, lectern, handhelds) | on | off | Put `voice` on the main speaker's mic. |
| Computer L / R, playback, walk-in music | off | on | They start with `duck` on. |
| Band, choir and instrument mics | off | off | Leave them out of both. |

`auto` and `duck` exclude each other: pressing one turns the other off. Never put `auto` on a
music strip: music is louder than speech and will take the gain from the mics.

Then set the faders for the balance you want as usual, and **store** a scene so the roles come
back next week.

## 3. During the service

| You see | It means |
|---|---|
| `mix 0` to `mix -3` on the talker's mic, green lamp | That mic has the gain and hears speech. |
| `mix -10` or lower on the other mics | Idle mics are down. Normal. |
| Readouts frozen when nobody talks | Last-mic hold: the last talker's mic stays open. |
| `duck -12` on the music strips | Music is dipped because a mic hears speech. It recovers about 1.5 s after the talking stops. |

- **Depth of the dip:** the `duck` knob under the strip's `duck` key (0–40 dB, default 12).
- **Take a mic out of auto-mix:** press its `auto` off. Muting it or pulling its fader all the way
  down also takes it out, and it comes back where it was when unmuted.
- **Stage monitors and `pfl` are not affected**, so the stage never pumps and you always hear the
  full mic on `pfl`.

## 4. If something is wrong

| Symptom | Cause | Fix |
|---|---|---|
| The talker's mic reads `mix -20` or lower | A music or noisy strip has `auto` on | Turn `auto` off on it; use `duck` for music. |
| Music never dips | No speech mic has `auto` on | Press `auto` on the speech mics. |
| Music dips when nobody talks | Loud music from the PA reaches a speech mic | Lower the PA near that mic, or reduce the duck depth. |
| The music pumps in and out | Depth too deep for the room | Set the `duck` knob to 6–9 dB. |
| Fewer strips than mics | The engine started on another input, or before the device change | Check Input in setup, then **stop** (twice) and **start**. |
| The sound stops and meters freeze mid-service | The output device dropped (often Bluetooth) | Stop and start the engine; use a wired monitor output. |

Design and engine details: `docs/specs/2026-11-19-automix-ducking-design.md`.
