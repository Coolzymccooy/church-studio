//! Which mixer strip carries what: the input device's channels first, then
//! the received NDI® sources (design decision 5). Shared by the engine (how
//! many strips it runs and meters) and `mixer_state` (how many it lists),
//! so both always agree.
use crate::mixer_control::MAX_STRIPS;
use crate::ndi::receive::MAX_NDI_INPUTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripLayout {
    /// Strips 0..hardware read input channels 0..hardware.
    pub hardware: usize,
    /// Strips hardware..hardware + ndi read the NDI inputs, in order.
    pub ndi: usize,
}

impl StripLayout {
    /// At least one hardware strip, at most `MAX_STRIPS`; NDI strips fill
    /// what is left, up to `MAX_NDI_INPUTS`.
    pub fn new(input_channels: usize, ndi_inputs: usize) -> Self {
        let hardware = input_channels.clamp(1, MAX_STRIPS);
        let ndi = ndi_inputs.min(MAX_NDI_INPUTS).min(MAX_STRIPS - hardware);
        StripLayout { hardware, ndi }
    }

    /// NDI inputs that fit after `input_channels` hardware strips.
    pub fn ndi_room(input_channels: usize) -> usize {
        Self::new(input_channels, MAX_NDI_INPUTS).ndi
    }

    pub fn total(&self) -> usize {
        self.hardware + self.ndi
    }

    /// Strip index of NDI input `slot`, if it has one.
    #[allow(dead_code)] // indexing helper; the engine assigns strips in order
    pub fn ndi_strip(&self, slot: usize) -> Option<usize> {
        (slot < self.ndi).then_some(self.hardware + slot)
    }

    #[allow(dead_code)]
    pub fn is_ndi(&self, strip: usize) -> bool {
        strip >= self.hardware && strip < self.total()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndi_strips_follow_the_hardware_channels() {
        let layout = StripLayout::new(2, 3);
        assert_eq!(layout, StripLayout { hardware: 2, ndi: 3 });
        assert_eq!(layout.total(), 5);
        assert_eq!(layout.ndi_strip(0), Some(2));
        assert_eq!(layout.ndi_strip(2), Some(4));
        assert_eq!(layout.ndi_strip(3), None);
        assert!(!layout.is_ndi(1));
        assert!(layout.is_ndi(2) && layout.is_ndi(4));
        assert!(!layout.is_ndi(5));
    }

    #[test]
    fn at_most_four_ndi_strips_and_never_past_max_strips() {
        assert_eq!(StripLayout::new(1, 9).ndi, MAX_NDI_INPUTS);
        assert_eq!(StripLayout::new(30, 4), StripLayout { hardware: 30, ndi: 2 });
        assert_eq!(StripLayout::new(64, 4), StripLayout { hardware: MAX_STRIPS, ndi: 0 });
        assert!(StripLayout::new(31, 4).total() <= MAX_STRIPS);
        assert_eq!(StripLayout::ndi_room(8), MAX_NDI_INPUTS);
        assert_eq!(StripLayout::ndi_room(31), 1);
    }

    #[test]
    fn no_device_channels_still_gives_one_strip() {
        assert_eq!(StripLayout::new(0, 0), StripLayout { hardware: 1, ndi: 0 });
        assert_eq!(StripLayout::new(0, 2).total(), 3);
    }
}
