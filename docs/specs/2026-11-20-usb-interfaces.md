# USB audio interfaces: hot-plug, reconnect, channel pairs, clocks

Operator requirement, 20 Nov 2026: TIWATON Studio must take audio in from a USB interface and
send audio out through one (Behringer UMC404HD, X-Air and similar), "effortlessly". Issue #65.

The engine already opens multichannel inputs (one strip per channel) and plays each bus on any
output device. This contract closes the gaps that made it fragile.

## 1. Device lists and names

- `list_input_devices` / `list_output_devices` return, per device: `id`, `name`, `is_default`
  (as before) plus `channels` (most channels the device offers in that direction), `transport`
  (`usb`, `builtin`, `bluetooth`, `virtual`, `aggregate`, `hdmi`, `thunderbolt`, `pci`,
  `network`, `other`) and `present: true`.
- Two devices with the same name (two identical interfaces) are told apart by order:
  the second is `"<name> (2)"`, the third `"<name> (3)"`. The same rule is used to open a device,
  so an id from the list always opens that device. A single device keeps its plain name.
- Ids stay names, so a saved choice finds the device again by name after a re-plug, a reboot, or
  a move to another USB port.

## 2. Hot-plug

- A device watcher thread runs for the life of the app. Every second it lists the input and output
  device names. When either list changes it emits `audio-devices-changed`
  `{ inputs: AudioDeviceInfo[], outputs: AudioDeviceInfo[] }`.
- The UI listens and refreshes its device lists. It never rewrites a saved device choice because the
  device is missing: the setup row keeps the saved name and shows a red `missing` lamp.
- `resetDeviceState` (permission failure) clears the device lists only, never the saved choices.

## 3. Device loss and reconnect while live

Each engine stream (the input and each output device) has a link state:

| state | meaning | lamp |
|---|---|---|
| `ok` | open and running | green `on` |
| `waiting` | chosen, but not connected when the engine started | amber `waiting` |
| `lost` | dropped out while running | red `lost` |
| `failed` | back, but it could not be reopened (the error says why) | red `failed` |

- A stream moves to `lost` when cpal reports `DeviceNotAvailable` or `StreamInvalidated`
  (the device's sample rate was changed elsewhere), or when the watcher has not listed it for two
  ticks in a row. A listing that fails keeps the previous lists: it is not an unplug.
- Each stream remembers the platform id of the device it opened (Core Audio UID, WASAPI endpoint
  id). A reconnect looks for that id first, then for the name, but never takes a device that
  another of the engine's streams was opened on (connected or not), so with two identical
  interfaces a bus cannot move onto the other unit. The system default resolves to its unique
  listed name ("X (2)" when it is the second unit).
- When a `lost` or `waiting` device has been listed again for two watcher ticks in a row, the
  engine reopens only that stream, by name, at the engine's sample rate. The watcher does not hold
  the engine lock while it waits for the reopen. The mixer, the other
  outputs, NDI, network audio and a running recording are not restarted.
- A failed reopen is retried on the next two ticks, then the stream stays `failed` until the
  device disappears and comes back (or the engine is restarted).
- An output that is not playing (`waiting`, `lost`, `failed`) is not fed, so its buffer does not
  fill or count drops; it starts empty when it plays again.
- An input that reopens with fewer channels than the strips need is `failed`
  ("has N channels; the input needs M").
- The input device must be connected to start the engine; a missing input is refused naming it
  (`Input 'X' is not connected`). A missing output does not stop the start: it is `waiting`
  and joins when plugged in.
- `engine_status.devices` lists every stream:
  `{ role, name, state, error, pair, channels, clock, drift_ppm, underruns, buffer_ms }` (see 4
  and 5). `buffer_ms` is the extra delay of that output's clock buffer (0 on the input's clock,
  about 16 ms at 48 kHz otherwise), for lip-sync downstream; this engine adds no other delay.
- Live service: plugging a device in changes no running stream; it is listed within about two
  seconds. Unplugging one device leaves every other output playing.
  `role` is `input`, `main`, `stream`, `monitor`, `monitor2`, `monitor3`.
- The event `audio-device-health` carries the same list whenever a state changes.
- The desk header shows one device lamp only when something needs attention:
  red `<name> lost` / `<name> failed`, amber `<name> waiting`, amber `clock slips`.
  Nothing is shown when every device is `ok`.

## 4. Channel pairs and input channels

- Each bus chooses an output pair on its device: `0` = 1-2, `1` = 3-4, `2` = 5-6, … (default 0).
  `start_audio_engine` takes `outputPairs: { main, stream, monitor, monitor2, monitor3 }`
  (missing keys are 0).
- Several buses may play on one device on different pairs (UMC404HD: main on 1-2, stream on
  3-4). One stream is opened per device; each bus writes its pair, every other channel is silent.
- Two buses on the same device and the same pair: broadcast / main are skipped (as before);
  mon 2 / mon 3 are refused with an error naming both outputs and the pair (as before, plus pair).
- A 1-channel device gets (L + R) / 2 on pair 0. A pair beyond the device's channels is not
  played: that bus's status is `failed` with "pair 3-4 is not on this device (2 channels)".
- The input takes `inputChannels: { first, count }` (0-based first channel; `count` null = to the
  last channel). Strip i reads device channel `first + i`. Hardware strips are named
  `Ch <first + i + 1>` while their name is automatic. A `first` past the device's channels is
  refused at start.
- The result of `start_audio_engine` and `engine_status` add `inputFirstChannel`.

## 5. Sample rate and clocks

- All devices still open at one common rate (the existing negotiation). When no common rate
  exists the error names each device with its rates, e.g.
  `No common sample rate: UMC404HD 192k (44.1-192 kHz), AirPods (24 kHz)`.
- An output device with the same name as the input device shares its clock (`clock: "shared"`):
  it is fed directly, as before.
- Any other output device (`clock: "corrected"`) runs on its own crystal. Its feed holds a small
  buffer (3 engine blocks, about 16 ms at 48 kHz) and is read at a ratio nudged by at most ±0.1 %
  (linear interpolation) to keep that buffer level, the way NDI and network inputs already are.
  `drift_ppm` reports the current nudge. A dry buffer plays silence and re-buffers; each such
  event counts in `underruns`. The header lamp shows `clock slips` when underruns rose in the
  last 10 s while no device was `lost` (a lost device makes every corrected output run dry once).
- Every output reads whole frames only, so a short buffer can never swap channels.

## 6. Setup panel (faceplate)

- Option text: `<name>` then ` · <n> ch` for devices with more than 2 channels, then ` · usb`
  for USB devices. A saved device that is missing stays selected, shown as `<name> · missing`.
- Each output row gets a pair select (`1-2`, `3-4`, …) shown only when its device has more than
  2 channels. The input row gets `first` and `count` selects shown only when the input has more
  than 2 channels.
- Row lamps follow the link state while the engine runs (`on`, `waiting`, `lost`, `failed`);
  the format strip adds a clock lamp: `clock shared`, `clock synced`, or amber `clock slips`.
- Pairs and channel choices are saved with the device choices and apply on the next start.
  Choosing another device puts its pair back to 1-2 (input: every channel). A saved pair or
  range the device does not have keeps its picker visible so it can be changed.
- Text stays sparse: explanations sit in ⓘ tips.

## Not in scope

- Resampling between devices that have no common rate (the error names them instead).
- Per-device buffer sizes; the engine block stays 256 frames.
