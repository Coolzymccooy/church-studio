# Three monitor mixes (mon 1, mon 2, mon 3)

Operator request, 20 Nov 2026: monitoring for up to three people at once (sound engineer, pastor,
worship leader), each with their own balance. Option B was chosen: three separate monitor mixes,
each to its own output device, like aux mixes on a hardware desk.

## Buses

| Index | Id | Label | Output device argument |
|---|---|---|---|
| 0 | `main` | main | `mainOutputId` |
| 1 | `stream` | stream | `broadcastOutputDevice` |
| 2 | `monitor` | mon (mon 1) | `monitorOutputDevice` |
| 3 | `monitor2` | mon 2 | `monitor2OutputId` (new, optional) |
| 4 | `monitor3` | mon 3 | `monitor3OutputId` (new, optional) |

- `BUS_MONITOR2 = 3`, `BUS_MONITOR3 = 4`, `BUS_COUNT = 5`. Every bus has the same parameters as
  today: `fader_db`, `mute`, `limiter_ceiling_db` (the limiter is always on).
- A bus whose device is not set is still mixed (cheap) but not sent anywhere.
- `mixer_state.buses[]` lists all five buses in the order above. Each bus gains
  `output: string | null`: the name of the device it plays on, or null while the engine is stopped
  or no device is set. The UI shows the mon 2 / mon 3 strips and send knobs only for buses whose
  device is set in setup.
- The monitor key (`monitor_gain_db`) scales `monitor` (mon 1) only. PFL / solo still goes to
  mon 1 only.

## Strip sends

- New strip params `send_monitor2_db`, `send_monitor3_db`: same range and parsing as the other
  sends (`FADER_MIN_DB..FADER_MAX_DB`, -90 is off). Default -90 (off), so a new mix starts silent.
- Mon 2 and mon 3 sends are always pre-fader (monitor mixes do not follow the front-of-house
  fader). `monitor_post_fader` keeps applying to mon 1 only.
- Strip mute mutes all sends, as today.
- Scenes store and restore the two new sends. Scene files written before this change load with
  -90 for both (serde default).

## Devices

- `start_audio_engine` takes two new optional args, `monitor2_output_id` and
  `monitor3_output_id`, opened like `main_output_id` with `pick_extra_output`, excluding every
  device already used by another output. Choosing the same device twice is refused with a clear
  error naming both outputs.
- `engine_status` adds `monitor2_output_name` and `monitor3_output_name` (null when not open).

## Meters

- `mixer-meters` `buses[]` carries all five buses (`id`, `peak_l_db`, `peak_r_db`, as today).

## UI (faceplate)

- Strip: a second sends row with `mon2` and `mon3` knobs (sends cap colour), shown only when that
  bus has a device.
- Buses: `mon 2` and `mon 3` bus strips beside `mon`, shown only when that bus has a device, each
  with a two-press "copy" key that copies every strip's mon 1 send into it (a starting point).
- Setup: Monitor 2 and Monitor 3 device rows with live level bars.
- NDI and the recorder are unchanged.
