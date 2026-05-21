use taffy::{
    GridTemplateComponent, GridTemplateRepetition, RepetitionCount, TrackSizingFunction,
    style_helpers,
};

pub enum Track {
    Single(TrackSizingFunction),
    Repeat(RepetitionCount, TrackSizingFunction),
}

impl Track {
    pub fn fr(flex: f32) -> Self {
        Track::Single(style_helpers::fr(flex))
    }

    pub fn px(px: f32) -> Self {
        Track::Single(style_helpers::length(px))
    }

    pub fn auto() -> Self {
        Track::Single(style_helpers::auto())
    }

    pub fn min_content() -> Self {
        Track::Single(style_helpers::min_content())
    }

    pub fn max_content() -> Self {
        Track::Single(style_helpers::max_content())
    }

    pub fn minmax(min: Track, max: Track) -> Self {
        let min_fn = match min {
            Track::Single(tsf) => tsf.min,
            _ => panic!("minmax min cannot be a repeat track"),
        };
        let max_fn = match max {
            Track::Single(tsf) => tsf.max,
            _ => panic!("minmax max cannot be a repeat track"),
        };
        Track::Single(style_helpers::minmax(min_fn, max_fn))
    }

    pub fn repeat(count: u16, track: Track) -> Self {
        Track::Repeat(RepetitionCount::Count(count), track.unwrap_single())
    }

    pub fn fill(track: Track) -> Self {
        Track::Repeat(RepetitionCount::AutoFill, track.unwrap_single())
    }

    pub fn fit(track: Track) -> Self {
        Track::Repeat(RepetitionCount::AutoFit, track.unwrap_single())
    }

    fn unwrap_single(self) -> TrackSizingFunction {
        match self {
            Track::Single(tsf) => tsf,
            _ => panic!("repeat tracks cannot be nested"),
        }
    }

    pub(crate) fn into_template_component(self) -> GridTemplateComponent<String> {
        match self {
            Track::Single(tsf) => GridTemplateComponent::Single(tsf),
            Track::Repeat(count, tsf) => GridTemplateComponent::Repeat(GridTemplateRepetition {
                count,
                tracks: vec![tsf],
                line_names: Vec::new(),
            }),
        }
    }

    pub(crate) fn into_auto_track(self) -> TrackSizingFunction {
        match self {
            Track::Single(tsf) => tsf,
            Track::Repeat(..) => panic!("repeat is not valid for grid_auto_rows/columns"),
        }
    }
}
