//! Grid track sizing: the `1fr`, `auto` and fixed forms a template column is written in.

use taffy::{
    GridTemplateComponent, GridTemplateRepetition, RepetitionCount, TrackSizingFunction,
    style_helpers,
};

/// One column or row of a grid template: a fixed length, `auto`, or a flexible `fr` share.
pub enum TemplateTrack {
    Single(TrackSizingFunction),
    Repeat(RepetitionCount, TrackSizingFunction),
}

impl TemplateTrack {
    pub fn fr(flex: f32) -> Self {
        TemplateTrack::Single(style_helpers::fr(flex))
    }

    pub fn px(px: f32) -> Self {
        TemplateTrack::Single(style_helpers::length(px))
    }

    pub fn auto() -> Self {
        TemplateTrack::Single(style_helpers::auto())
    }

    pub fn minmax(min: TemplateTrack, max: TemplateTrack) -> Self {
        let min_fn = match min {
            TemplateTrack::Single(tsf) => tsf.min,
            _ => panic!("minmax min cannot be a repeat track"),
        };
        let max_fn = match max {
            TemplateTrack::Single(tsf) => tsf.max,
            _ => panic!("minmax max cannot be a repeat track"),
        };
        TemplateTrack::Single(style_helpers::minmax(min_fn, max_fn))
    }

    pub fn repeat(count: u16, track: TemplateTrack) -> Self {
        TemplateTrack::Repeat(RepetitionCount::Count(count), track.unwrap_single())
    }

    pub fn fill(track: TemplateTrack) -> Self {
        TemplateTrack::Repeat(RepetitionCount::AutoFill, track.unwrap_single())
    }

    pub fn fit(track: TemplateTrack) -> Self {
        TemplateTrack::Repeat(RepetitionCount::AutoFit, track.unwrap_single())
    }

    fn unwrap_single(self) -> TrackSizingFunction {
        match self {
            TemplateTrack::Single(tsf) => tsf,
            _ => panic!("repeat tracks cannot be nested"),
        }
    }

    pub(crate) fn into_template_component(self) -> GridTemplateComponent<String> {
        match self {
            TemplateTrack::Single(tsf) => GridTemplateComponent::Single(tsf),
            TemplateTrack::Repeat(count, tsf) => {
                GridTemplateComponent::Repeat(GridTemplateRepetition {
                    count,
                    tracks: vec![tsf],
                    line_names: Vec::new(),
                })
            }
        }
    }
}
