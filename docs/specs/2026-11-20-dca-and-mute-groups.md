# DCA and mute groups

## Signal flow

The Groups bank provides eight DCA faders and eight independent mute-group keys. A channel strip can belong to any number of DCAs and mute groups. A DCA adjusts the effective post-fader gain of its members while keeping each channel's fader value and position. Main, Stream, and Mon 1 in post-fader mode follow the DCA gain. Pre-fader monitor sends and PFL keep their own levels.

Each DCA also has a mute. An active DCA mute or mute group mutes every member's output sends. PFL retains its normal ability to audition muted channels. A strip with more than one mute reason stays muted until all its reasons clear. Group gain and mute changes use the existing 10 ms ramps. Groups do not share or sum audio and add no signal latency.

## Parameters

| Control | Range | Default |
| --- | --- | --- |
| DCA number and mute group number | 1–8 on the desk, 0–7 in commands | All eight available |
| DCA fader | −90 to +10 dB. −90 is silence | 0 dB |
| DCA mute and mute group | On or off | Off |
| Strip membership in each group | Assigned or clear | Clear |

Multiple DCA gains add to the member fader in dB. The final combined post-fader gain is capped at +10 dB. A DCA at −90 dB silences its post-fader paths regardless of any other group gain. Input groups use existing strip indices, including active network, computer and NDI inputs. Output-bus masters keep their own independent controls.

## Commands, state and persistence

`mixer_set_group(change)` accepts a typed `fader`, `dcaMute`, `muteGroup`, or `membership` change. Membership includes `kind` (`dca` or `mute`), `index` (group number), `strip` (mixer index), and `enabled`. The desk uses the existing coalesced update path. The remote tablet uses dedicated DCA fader and mute setters. `rename` stores a group name of up to 24 characters. Empty names use the numbered defaults. Bad group or strip indices are rejected, and finite fader values are clamped. Engine processing reads the group parameters once per block and never allocates or locks for groups.

`mixer_state.groups` reports the eight faders, DCA mute mask and active mute-group mask. Each strip reports its membership masks and effective group mute state. Scene load and state refresh carry these fields alongside the existing mixer state. Group masters and memberships publish as one generation at recall, and a racing audio read keeps the previous complete generation. Scenes save group names, faders, mutes and memberships. A legacy scene resets these controls to defaults. A newly assigned input feed starts with clear membership. Existing input feeds keep their memberships when the layout moves them.

The Groups bank shows eight faders, mute keys and post-fader member peak meters. One Assign control opens the naming and membership controls for either group kind. Input strips reserve a fixed row for group state, show the effective DCA change, and list membership in an InfoTip. A group mute or fully closed DCA dims the scribble strip.

Verify that member gains track the DCA without moving the member faders, non-members and pre-fader sends stay at their old levels, overlapping mute reasons release independently, and scene migration and recall restore the whole assignment. Check the desk at desktop width in both finishes.
