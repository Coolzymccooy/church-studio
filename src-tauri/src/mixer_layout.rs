//! Which mixer strip carries what: the input device's channels first, then
//! the computer-audio pair (when it is on), then the received NDI® sources
//! (design decision 5). Shared by the engine (how many strips it runs and
//! meters) and `mixer_state` (how many it lists), so both always agree.
//!
//! The NDI strips stay the last entries, so `ndiStrips` keeps meaning "the
//! last n strips" for every client that already reads it.
use crate::mixer_control::MAX_STRIPS;
use crate::ndi::receive::MAX_NDI_INPUTS;

/// Strips the computer-audio input takes when it is on: left, then right.
pub const COMPUTER_STRIPS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripLayout {
    /// Strips 0..hardware read input channels 0..hardware.
    pub hardware: usize,
    /// Strips hardware..hardware + computer carry computer audio (0 or 2:
    /// left, then right).
    pub computer: usize,
    /// The next `ndi` strips read the NDI inputs, in order.
    pub ndi: usize,
}

impl StripLayout {
    /// At least one hardware strip, at most `MAX_STRIPS`. The computer pair
    /// comes next when it is on and both of its strips fit; NDI strips fill
    /// what is left, up to `MAX_NDI_INPUTS`.
    pub fn new(input_channels: usize, computer: bool, ndi_inputs: usize) -> Self {
        let hardware = input_channels.clamp(1, MAX_STRIPS);
        let computer = if computer && MAX_STRIPS - hardware >= COMPUTER_STRIPS {
            COMPUTER_STRIPS
        } else {
            0
        };
        let ndi = ndi_inputs
            .min(MAX_NDI_INPUTS)
            .min(MAX_STRIPS - hardware - computer);
        StripLayout {
            hardware,
            computer,
            ndi,
        }
    }

    /// The computer pair fits after `input_channels` hardware strips.
    pub fn computer_fits(input_channels: usize) -> bool {
        Self::new(input_channels, true, 0).computer == COMPUTER_STRIPS
    }

    /// NDI inputs that fit after the hardware strips and the computer pair.
    pub fn ndi_room(input_channels: usize, computer: bool) -> usize {
        Self::new(input_channels, computer, MAX_NDI_INPUTS).ndi
    }

    pub fn total(&self) -> usize {
        self.hardware + self.computer + self.ndi
    }

    /// First NDI strip.
    pub fn first_ndi_strip(&self) -> usize {
        self.hardware + self.computer
    }

    /// The computer pair's (left, right) strips, when it is on.
    pub fn computer_strips(&self) -> Option<(usize, usize)> {
        (self.computer == COMPUTER_STRIPS).then_some((self.hardware, self.hardware + 1))
    }

    /// Strip index of NDI input `slot`, if it has one.
    #[allow(dead_code)] // indexing helper; the engine assigns strips in order
    pub fn ndi_strip(&self, slot: usize) -> Option<usize> {
        (slot < self.ndi).then_some(self.first_ndi_strip() + slot)
    }

    #[allow(dead_code)]
    pub fn is_ndi(&self, strip: usize) -> bool {
        strip >= self.first_ndi_strip() && strip < self.total()
    }

    #[allow(dead_code)]
    pub fn is_computer(&self, strip: usize) -> bool {
        strip >= self.hardware && strip < self.first_ndi_strip()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndi_strips_follow_the_hardware_channels() {
        let layout = StripLayout::new(2, false, 3);
        assert_eq!(layout, StripLayout { hardware: 2, computer: 0, ndi: 3 });
        assert_eq!(layout.total(), 5);
        assert_eq!(layout.ndi_strip(0), Some(2));
        assert_eq!(layout.ndi_strip(2), Some(4));
        assert_eq!(layout.ndi_strip(3), None);
        assert!(!layout.is_ndi(1));
        assert!(layout.is_ndi(2) && layout.is_ndi(4));
        assert!(!layout.is_ndi(5));
        assert_eq!(layout.computer_strips(), None);
        assert!(!layout.is_computer(2));
    }

    #[test]
    fn computer_pair_sits_between_hardware_and_ndi() {
        let layout = StripLayout::new(2, true, 2);
        assert_eq!(layout, StripLayout { hardware: 2, computer: 2, ndi: 2 });
        assert_eq!(layout.total(), 6);
        assert_eq!(layout.computer_strips(), Some((2, 3)));
        assert!(layout.is_computer(2) && layout.is_computer(3));
        assert!(!layout.is_computer(1) && !layout.is_computer(4));
        // NDI stays the last strips.
        assert_eq!(layout.first_ndi_strip(), 4);
        assert_eq!(layout.ndi_strip(0), Some(4));
        assert_eq!(layout.ndi_strip(1), Some(5));
        assert!(layout.is_ndi(4) && layout.is_ndi(5) && !layout.is_ndi(3));
    }

    #[test]
    fn computer_pair_needs_room_for_both_strips() {
        assert_eq!(StripLayout::new(30, true, 0).computer, 2);
        assert_eq!(StripLayout::new(31, true, 0).computer, 0);
        assert_eq!(StripLayout::new(64, true, 4).total(), MAX_STRIPS);
        assert!(StripLayout::computer_fits(30));
        assert!(!StripLayout::computer_fits(31));
        // The pair takes room from NDI, never the other way round.
        assert_eq!(StripLayout::new(28, true, 4), StripLayout { hardware: 28, computer: 2, ndi: 2 });
        assert_eq!(StripLayout::ndi_room(28, true), 2);
        assert_eq!(StripLayout::ndi_room(28, false), MAX_NDI_INPUTS);
    }

    #[test]
    fn at_most_four_ndi_strips_and_never_past_max_strips() {
        assert_eq!(StripLayout::new(1, false, 9).ndi, MAX_NDI_INPUTS);
        assert_eq!(StripLayout::new(30, false, 4), StripLayout { hardware: 30, computer: 0, ndi: 2 });
        assert_eq!(StripLayout::new(64, false, 4), StripLayout { hardware: MAX_STRIPS, computer: 0, ndi: 0 });
        assert!(StripLayout::new(31, true, 4).total() <= MAX_STRIPS);
        assert_eq!(StripLayout::ndi_room(8, false), MAX_NDI_INPUTS);
        assert_eq!(StripLayout::ndi_room(31, false), 1);
    }

    #[test]
    fn no_device_channels_still_gives_one_strip() {
        assert_eq!(StripLayout::new(0, false, 0), StripLayout { hardware: 1, computer: 0, ndi: 0 });
        assert_eq!(StripLayout::new(0, false, 2).total(), 3);
        assert_eq!(StripLayout::new(0, true, 0).computer_strips(), Some((1, 2)));
    }
}
