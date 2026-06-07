use taffy::{
    GridTemplateComponent, GridTemplateRepetition, RepetitionCount, TrackSizingFunction,
    style_helpers,
};

pub trait TrackSizing: Sized {
    fn fr(flex: f32) -> Self;
    fn px(px: f32) -> Self;
    fn auto() -> Self;
    fn min_content() -> Self;
    fn max_content() -> Self;
    fn minmax(min: Self, max: Self) -> Self;
}

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

    pub fn min_content() -> Self {
        TemplateTrack::Single(style_helpers::min_content())
    }

    pub fn max_content() -> Self {
        TemplateTrack::Single(style_helpers::max_content())
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

impl TrackSizing for TemplateTrack {
    fn fr(flex: f32) -> Self {
        TemplateTrack::fr(flex)
    }
    fn px(px: f32) -> Self {
        TemplateTrack::px(px)
    }
    fn auto() -> Self {
        TemplateTrack::auto()
    }
    fn min_content() -> Self {
        TemplateTrack::min_content()
    }
    fn max_content() -> Self {
        TemplateTrack::max_content()
    }
    fn minmax(min: Self, max: Self) -> Self {
        TemplateTrack::minmax(min, max)
    }
}

pub struct AutoTrack(TrackSizingFunction);

impl AutoTrack {
    pub fn fr(flex: f32) -> Self {
        AutoTrack(style_helpers::fr(flex))
    }

    pub fn px(px: f32) -> Self {
        AutoTrack(style_helpers::length(px))
    }

    pub fn auto() -> Self {
        AutoTrack(style_helpers::auto())
    }

    pub fn min_content() -> Self {
        AutoTrack(style_helpers::min_content())
    }

    pub fn max_content() -> Self {
        AutoTrack(style_helpers::max_content())
    }

    pub fn minmax(min: AutoTrack, max: AutoTrack) -> Self {
        AutoTrack(style_helpers::minmax(min.0.min, max.0.max))
    }

    pub(crate) fn into_sizing_function(self) -> TrackSizingFunction {
        self.0
    }
}

impl TrackSizing for AutoTrack {
    fn fr(flex: f32) -> Self {
        AutoTrack::fr(flex)
    }
    fn px(px: f32) -> Self {
        AutoTrack::px(px)
    }
    fn auto() -> Self {
        AutoTrack::auto()
    }
    fn min_content() -> Self {
        AutoTrack::min_content()
    }
    fn max_content() -> Self {
        AutoTrack::max_content()
    }
    fn minmax(min: Self, max: Self) -> Self {
        AutoTrack::minmax(min, max)
    }
}
