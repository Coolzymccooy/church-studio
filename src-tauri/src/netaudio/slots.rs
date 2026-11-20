//! Which remote peer feeds which Remote strip (decision 4).
//!
//! A peer is one sender run: its address plus its random session id. A new
//! peer takes the first free slot. A sender that restarts (same IP, new
//! session) while its old session has been silent for `TAKEOVER_MS` takes
//! the old slot back, so a remote preacher stays on the same strip. A slot
//! frees on `bye` or after `TIMEOUT_MS` without packets. Times are
//! milliseconds on any monotonic clock.
use std::net::SocketAddr;

/// A slot frees after this long without packets.
pub const TIMEOUT_MS: u64 = 3_000;
/// A restarted sender may take its old slot once the old session is this
/// quiet.
pub const TAKEOVER_MS: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// The packet belongs to the peer already in this slot.
    Existing(usize),
    /// A new peer now holds this slot (reset its stream).
    New(usize),
    /// No slot is free.
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Holder {
    addr: SocketAddr,
    session: u32,
    last_heard_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SlotTable {
    slots: Vec<Option<Holder>>,
}

impl SlotTable {
    pub fn new(count: usize) -> Self {
        SlotTable {
            slots: vec![None; count],
        }
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// The slot for a signed packet from (`addr`, `session`), at `now_ms`.
    pub fn claim(&mut self, addr: SocketAddr, session: u32, now_ms: u64) -> Claim {
        if let Some(index) = self.find(addr, session) {
            self.touch(index, now_ms);
            return Claim::Existing(index);
        }
        let takeover = self.slots.iter().position(|slot| {
            slot.is_some_and(|held| {
                held.addr.ip() == addr.ip()
                    && now_ms.saturating_sub(held.last_heard_ms) >= TAKEOVER_MS
            })
        });
        let Some(index) = takeover.or_else(|| self.slots.iter().position(Option::is_none)) else {
            return Claim::Full;
        };
        self.slots[index] = Some(Holder {
            addr,
            session,
            last_heard_ms: now_ms,
        });
        Claim::New(index)
    }

    /// The slot this peer holds, if any (does not refresh it).
    pub fn find(&self, addr: SocketAddr, session: u32) -> Option<usize> {
        self.slots.iter().position(|slot| {
            slot.is_some_and(|held| held.addr == addr && held.session == session)
        })
    }

    fn touch(&mut self, index: usize, now_ms: u64) {
        if let Some(held) = self.slots[index].as_mut() {
            held.last_heard_ms = now_ms;
        }
    }

    /// The peer said goodbye: free its slot.
    pub fn release(&mut self, addr: SocketAddr, session: u32) -> Option<usize> {
        let index = self.find(addr, session)?;
        self.slots[index] = None;
        Some(index)
    }

    /// Free every slot quiet for `TIMEOUT_MS`; calls `freed` with each.
    pub fn expire(&mut self, now_ms: u64, mut freed: impl FnMut(usize)) {
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_some_and(|held| now_ms.saturating_sub(held.last_heard_ms) >= TIMEOUT_MS) {
                *slot = None;
                freed(index);
            }
        }
    }

    /// (slot, address, session) of every held slot.
    pub fn held(&self) -> impl Iterator<Item = (usize, SocketAddr, u32)> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.map(|held| (index, held.addr, held.session)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    #[test]
    fn peers_fill_slots_in_order_and_keep_them() {
        let mut table = SlotTable::new(2);
        assert_eq!(table.claim(addr("10.0.0.1:5000"), 1, 0), Claim::New(0));
        assert_eq!(table.claim(addr("10.0.0.2:5000"), 2, 0), Claim::New(1));
        assert_eq!(table.claim(addr("10.0.0.1:5000"), 1, 10), Claim::Existing(0));
        assert_eq!(table.claim(addr("10.0.0.3:5000"), 3, 10), Claim::Full);
    }

    #[test]
    fn two_live_senders_on_one_machine_get_two_slots() {
        let mut table = SlotTable::new(2);
        assert_eq!(table.claim(addr("10.0.0.1:5000"), 1, 0), Claim::New(0));
        assert_eq!(table.claim(addr("10.0.0.1:5001"), 2, 100), Claim::New(1));
    }

    #[test]
    fn a_restarted_sender_takes_its_old_slot_back() {
        let mut table = SlotTable::new(2);
        table.claim(addr("10.0.0.2:5000"), 7, 0);
        table.claim(addr("10.0.0.1:5000"), 1, 0);
        // Same machine, new port and session, old session quiet.
        assert_eq!(table.claim(addr("10.0.0.1:6000"), 9, TAKEOVER_MS), Claim::New(1));
        assert_eq!(table.find(addr("10.0.0.1:5000"), 1), None);
    }

    #[test]
    fn bye_and_timeout_free_slots() {
        let mut table = SlotTable::new(2);
        table.claim(addr("10.0.0.1:5000"), 1, 0);
        table.claim(addr("10.0.0.2:5000"), 2, 0);
        assert_eq!(table.release(addr("10.0.0.1:5000"), 1), Some(0));
        assert_eq!(table.release(addr("10.0.0.1:5000"), 1), None);
        let mut freed = Vec::new();
        table.expire(TIMEOUT_MS - 1, |slot| freed.push(slot));
        assert!(freed.is_empty());
        table.expire(TIMEOUT_MS, |slot| freed.push(slot));
        assert_eq!(freed, vec![1]);
        assert_eq!(table.held().count(), 0);
    }
}
