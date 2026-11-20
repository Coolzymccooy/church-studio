use crate::mixer_layout::StripLayout;
use crate::recorder::session::SessionTrack;
use serde::Serialize;

pub struct PlaybackInput {
    pub index: usize,
    pub name: String,
    pub source: &'static str,
}

impl PlaybackInput {
    pub fn for_strip(index: usize, name: String, layout: StripLayout) -> Self {
        let source = if layout.is_ndi(index) {
            "ndi"
        } else if layout.is_computer(index) {
            "computer"
        } else if layout.is_net(index) {
            "net"
        } else {
            "hardware"
        };
        Self {
            index,
            name,
            source,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct MappingWarning {
    pub name: String,
    pub reason: &'static str,
}

pub(super) fn matching_input<'a>(
    track: &SessionTrack,
    inputs: &'a [PlaybackInput],
    index: usize,
) -> Result<&'a PlaybackInput, MappingWarning> {
    let by_name =
        |input: &&PlaybackInput| input.name.trim().eq_ignore_ascii_case(track.name.trim());
    let mut matches = inputs
        .iter()
        .filter(by_name)
        .filter(|input| input.source == track.source);
    if let Some(first) = matches.next() {
        if let Some(second) = matches.next() {
            if let Some(exact) = std::iter::once(first)
                .chain(std::iter::once(second))
                .chain(matches)
                .find(|input| input.index == index)
            {
                return Ok(exact);
            }
            return Err(warning(track, "Ambiguous strip name"));
        }
        return Ok(first);
    }
    let original = inputs.iter().find(|input| input.index == index);
    let reason = if inputs.iter().any(|input| by_name(&input))
        || original.is_some_and(|input| input.source != track.source)
    {
        "Source kind differs"
    } else if original.is_some() {
        "Strip name differs"
    } else {
        "Strip unavailable"
    };
    Err(warning(track, reason))
}

pub(super) fn warning(track: &SessionTrack, reason: &'static str) -> MappingWarning {
    MappingWarning {
        name: track.name.clone(),
        reason,
    }
}
