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

Multiple DCA gains combine in dB, limited to +10 dB of combined DCA boost per strip. A DCA at −90 dB silences its post-fader paths regardless of any other group gain. Input groups use existing strip indices, including active network, computer and NDI inputs. Output-bus masters keep their own independent controls.

## Commands, state and persistence

`mixer_set_group(change)` accepts a typed `fader`, `dcaMute`, `muteGroup`, or `membership` change. Membership includes `kind` (`dca` or `mute`), `index` (group number), `strip` (mixer index), and `enabled`. The desk uses the existing coalesced update path. The shared mixer setters also accept `dca_fader_db` and `dca_mute` by group index for the remote tablet. Bad group or strip indices are rejected, and finite fader values are clamped. Engine processing reads the group parameters once per block and never allocates or locks for groups.

`mixer_state.groups` reports the eight faders, DCA mute mask and active mute-group mask. Each strip reports its membership masks and effective group mute state. Scene load and state refresh carry these fields alongside the existing mixer state. Scenes save group faders, mutes and memberships. A legacy scene resets these controls to defaults. A newly assigned input feed starts with clear membership. Existing input feeds keep their memberships when the layout moves them.

The Groups bank shows the eight faders together with their mute and Assign keys. Mute groups have a separate row of keys. The assignment pane shows named input strips and uses pressed keys for membership. An input scribble strip dims on a group mute, with its cause in an InfoTip.

Verify that member gains track the DCA without moving the member faders, non-members and pre-fader sends stay at their old levels, overlapping mute reasons release independently, and scene migration and recall restore the whole assignment. Check the desk at desktop width in both finishes.
