//! Publish the group masters and memberships as a single generation.
//! The audio thread reads at most once per block and holds its last complete
//! snapshot if the control thread overtakes the read.
use super::groups::{GroupState, Membership, GROUP_COUNT};
use super::params::StripParams;
use crate::mixer_control::MAX_STRIPS;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering::SeqCst};

#[derive(Clone, Copy)]
pub(super) struct GroupSnapshot {
    generation: u64,
    pub(super) state: GroupState,
    pub(super) members: [Membership; MAX_STRIPS],
}

impl Default for GroupSnapshot {
    fn default() -> Self {
        Self {
            generation: 0,
            state: GroupState::default(),
            members: [Membership::default(); MAX_STRIPS],
        }
    }
}

#[derive(Default)]
struct Bank {
    faders: [AtomicU32; GROUP_COUNT],
    flags: AtomicU16,
    members: [AtomicU16; MAX_STRIPS],
}

impl Bank {
    fn write(&self, state: GroupState, strips: &[StripParams]) {
        for (field, db) in self.faders.iter().zip(state.faders) {
            field.store(db.to_bits(), SeqCst);
        }
        self.flags.store(
            u16::from_le_bytes([state.dca_muted, state.mute_active]),
            SeqCst,
        );
        for (i, cell) in self.members.iter().enumerate() {
            let m = strips
                .get(i)
                .map(|p| p.groups.capture())
                .unwrap_or_default();
            cell.store(u16::from_le_bytes([m.dca, m.mute]), SeqCst);
        }
    }

    fn read(&self, generation: u64) -> GroupSnapshot {
        let [dca_muted, mute_active] = self.flags.load(SeqCst).to_le_bytes();
        GroupSnapshot {
            generation,
            state: GroupState {
                faders: self
                    .faders
                    .each_ref()
                    .map(|f| f32::from_bits(f.load(SeqCst))),
                dca_muted,
                mute_active,
            },
            members: self.members.each_ref().map(|m| {
                let [dca, mute] = m.load(SeqCst).to_le_bytes();
                Membership { dca, mute }
            }),
        }
    }
}

#[derive(Default)]
pub(super) struct GroupPublication {
    active: AtomicU64,
    banks: [Bank; 2],
}

impl GroupPublication {
    // The caller owns the group edit lock. The inactive bank may still be
    // observed by a reader of an older generation, which will reject it.
    pub(super) fn publish(&self, state: GroupState, strips: &[StripParams]) {
        let next = self.active.load(SeqCst).wrapping_add(1);
        self.banks[(next & 1) as usize].write(state, strips);
        self.active.store(next, SeqCst);
    }

    pub(super) fn refresh(&self, cached: &mut GroupSnapshot) {
        let generation = self.active.load(SeqCst);
        if generation == cached.generation {
            return;
        }
        let candidate = self.banks[(generation & 1) as usize].read(generation);
        if self.active.load(SeqCst) == generation {
            *cached = candidate;
        }
    }
}
