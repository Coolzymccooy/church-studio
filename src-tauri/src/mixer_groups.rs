use crate::dsp::mixer::groups::GROUP_COUNT;
use crate::dsp::mixer::params::{clamp_or, FADER_MAX_DB, MIN_DB};
use crate::mixer_control::MixerControl;
use serde::Deserialize;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};
use tauri::State;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Clone, Copy)]
pub enum GroupKind {
    Dca,
    Mute,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum GroupChange {
    Fader {
        index: usize,
        db: f32,
    },
    DcaMute {
        index: usize,
        enabled: bool,
    },
    MuteGroup {
        index: usize,
        enabled: bool,
    },
    Rename {
        index: usize,
        kind: GroupKind,
        name: String,
    },
    Membership {
        index: usize,
        strip: usize,
        kind: GroupKind,
        enabled: bool,
    },
}

fn set_bit(mask: &AtomicU8, index: usize, enabled: bool) -> Result<(), String> {
    if index >= GROUP_COUNT {
        return Err("Invalid group number".into());
    }
    let bit = 1u8 << index;
    if enabled {
        mask.fetch_or(bit, Relaxed);
    } else {
        mask.fetch_and(!bit, Relaxed);
    }
    Ok(())
}

pub fn set_group(mixer: &MixerControl, change: GroupChange) -> Result<(), String> {
    let groups = &mixer.params.groups;
    let _edit = groups.edit.lock();
    let result: Result<(), String> = match change {
        GroupChange::Fader { index, db } => {
            let fader = groups.faders.get(index).ok_or("Invalid group number")?;
            fader.store(clamp_or(db, MIN_DB, FADER_MAX_DB, 0.0), Relaxed);
            Ok(())
        }
        GroupChange::DcaMute { index, enabled } => set_bit(&groups.dca_muted, index, enabled),
        GroupChange::MuteGroup { index, enabled } => set_bit(&groups.mute_active, index, enabled),
        GroupChange::Rename { index, kind, name } => {
            let name = name.trim();
            if name.chars().count() > crate::mixer_control::MAX_STRIP_NAME_CHARS
                || name.chars().any(char::is_control)
            {
                return Err("Invalid group name".into());
            }
            let mut labels = groups.labels.lock();
            let names = match kind {
                GroupKind::Dca => &mut labels.dca,
                GroupKind::Mute => &mut labels.mute,
            };
            *names.get_mut(index).ok_or("Invalid group number")? = name.to_owned();
            Ok(())
        }
        GroupChange::Membership {
            index,
            strip,
            kind,
            enabled,
        } => {
            let member = &mixer
                .params
                .strips
                .get(strip)
                .ok_or("Invalid strip number")?
                .groups;
            let mask = match kind {
                GroupKind::Dca => &member.dca,
                GroupKind::Mute => &member.mute,
            };
            set_bit(mask, index, enabled)
        }
    };
    result?;
    groups.publish(&mixer.params.strips);
    Ok(())
}

impl MixerControl {
    pub fn set_dca_fader(&self, index: usize, db: f32) -> Result<(), String> {
        set_group(self, GroupChange::Fader { index, db })
    }

    pub fn set_dca_mute(&self, index: usize, enabled: bool) -> Result<(), String> {
        set_group(self, GroupChange::DcaMute { index, enabled })
    }
}

#[tauri::command]
pub fn mixer_set_group(mixer: State<'_, MixerControl>, change: GroupChange) -> Result<(), String> {
    set_group(mixer.inner(), change)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mixer_layout::StripLayout;

    #[test]
    fn validates_group_changes_and_reports_effective_mute() {
        let mixer = MixerControl::new();
        assert!(set_group(&mixer, GroupChange::Fader { index: 8, db: 5.0 }).is_err());
        assert!(set_group(
            &mixer,
            GroupChange::Membership {
                index: 0,
                strip: 32,
                kind: GroupKind::Mute,
                enabled: true,
            }
        )
        .is_err());
        set_group(&mixer, GroupChange::Fader { index: 0, db: 50.0 }).unwrap();
        set_group(
            &mixer,
            GroupChange::Membership {
                index: 0,
                strip: 0,
                kind: GroupKind::Mute,
                enabled: true,
            },
        )
        .unwrap();
        set_group(
            &mixer,
            GroupChange::MuteGroup {
                index: 0,
                enabled: true,
            },
        )
        .unwrap();
        mixer.set_dca_fader(7, -8.0).unwrap();
        mixer.set_dca_mute(7, true).unwrap();
        set_group(
            &mixer,
            GroupChange::Rename {
                index: 0,
                kind: GroupKind::Mute,
                name: "Talk mics".into(),
            },
        )
        .unwrap();
        let saved = mixer.capture_scene("Named groups");
        let restored = MixerControl::new();
        restored.apply_scene(&saved);
        assert_eq!(
            restored
                .capture_scene("Named groups")
                .mixer
                .group_labels
                .mute[0],
            "Talk mics"
        );
        let state = mixer.snapshot_layout(StripLayout::new(2, false, 0), true, vec![]);
        assert_eq!(state.groups.faders[0], 10.0);
        assert_eq!(state.groups.faders[7], -8.0);
        assert_eq!(state.groups.dca_muted, 128);
        assert!(state.strips[0].group_muted && !state.strips[1].group_muted);
        assert!(!state.strips[0].mute);
    }
}
