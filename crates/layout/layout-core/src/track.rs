//! Grid track sizing: the `1fr`, `auto` and fixed forms a template column is written in.

use geometry_core::Size;
use taffy::{
    GridTemplateComponent, GridTemplateRepetition, MaxTrackSizingFunction, MinTrackSizingFunction,
    RepetitionCount, TrackSizingFunction,
};

use crate::style::SizeDimension;

/// One bound of a track, kept as written so a fraction of the surface can be resolved again when the surface changes size.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Breadth {
    Length(SizeDimension),
    Fr(f32),
    Auto,
}

impl Breadth {
    fn min(self, surface: Size) -> MinTrackSizingFunction {
        match self {
            Breadth::Length(d) => match d.against(surface) {
                SizeDimension::Px(v) => MinTrackSizingFunction::length(v),
                SizeDimension::Percent(v) => MinTrackSizingFunction::percent(v),
                _ => MinTrackSizingFunction::auto(),
            },
            // CSS has no flexible minimum: a `1fr` track's floor is `auto`.
            Breadth::Fr(_) | Breadth::Auto => MinTrackSizingFunction::auto(),
        }
    }

    fn max(self, surface: Size) -> MaxTrackSizingFunction {
        match self {
            Breadth::Length(d) => match d.against(surface) {
                SizeDimension::Px(v) => MaxTrackSizingFunction::length(v),
                SizeDimension::Percent(v) => MaxTrackSizingFunction::percent(v),
                _ => MaxTrackSizingFunction::auto(),
            },
            Breadth::Fr(flex) => MaxTrackSizingFunction::fr(flex),
            Breadth::Auto => MaxTrackSizingFunction::auto(),
        }
    }

    fn is_surface_relative(self) -> bool {
        matches!(self, Breadth::Length(d) if d.is_surface_relative())
    }
}

/// A track's floor and ceiling, as written.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sizing {
    min: Breadth,
    max: Breadth,
}

impl Sizing {
    fn resolve(self, surface: Size) -> TrackSizingFunction {
        TrackSizingFunction {
            min: self.min.min(surface),
            max: self.max.max(surface),
        }
    }

    fn is_surface_relative(self) -> bool {
        self.min.is_surface_relative() || self.max.is_surface_relative()
    }
}

/// One column or row of a grid template: a fixed length, a fraction of the surface, `auto`, or a flexible `fr` share.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateTrack(Track);

#[derive(Clone, Debug, PartialEq)]
enum Track {
    Single(Sizing),
    Repeat(RepetitionCount, Sizing),
}

impl TemplateTrack {
    fn single(breadth: Breadth) -> Self {
        TemplateTrack(Track::Single(Sizing {
            min: breadth,
            max: breadth,
        }))
    }

    pub fn fr(flex: f32) -> Self {
        Self::single(Breadth::Fr(flex))
    }

    pub fn px(px: f32) -> Self {
        Self::length(SizeDimension::Px(px))
    }

    /// A fixed track of any length a size takes — pixels, a percentage of the grid, or a fraction of the surface. `auto` is [`auto`](Self::auto).
    pub fn length(length: SizeDimension) -> Self {
        match length {
            SizeDimension::Auto => Self::auto(),
            other => Self::single(Breadth::Length(other)),
        }
    }

    pub fn auto() -> Self {
        Self::single(Breadth::Auto)
    }

    pub fn minmax(min: TemplateTrack, max: TemplateTrack) -> Self {
        let min = match min.0 {
            Track::Single(sizing) => sizing.min,
            _ => panic!("minmax min cannot be a repeat track"),
        };
        let max = match max.0 {
            Track::Single(sizing) => sizing.max,
            _ => panic!("minmax max cannot be a repeat track"),
        };
        TemplateTrack(Track::Single(Sizing { min, max }))
    }

    pub fn repeat(count: u16, track: TemplateTrack) -> Self {
        TemplateTrack(Track::Repeat(
            RepetitionCount::Count(count),
            track.unwrap_single(),
        ))
    }

    pub fn fill(track: TemplateTrack) -> Self {
        TemplateTrack(Track::Repeat(
            RepetitionCount::AutoFill,
            track.unwrap_single(),
        ))
    }

    pub fn fit(track: TemplateTrack) -> Self {
        TemplateTrack(Track::Repeat(
            RepetitionCount::AutoFit,
            track.unwrap_single(),
        ))
    }

    fn unwrap_single(self) -> Sizing {
        match self.0 {
            Track::Single(sizing) => sizing,
            _ => panic!("repeat tracks cannot be nested"),
        }
    }

    /// Whether this track names a fraction of the surface, and so has to be resolved again when the surface changes size.
    pub(crate) fn is_surface_relative(&self) -> bool {
        match &self.0 {
            Track::Single(sizing) | Track::Repeat(_, sizing) => sizing.is_surface_relative(),
        }
    }

    /// For `grid_auto_rows`/`grid_auto_columns`, which — like CSS's `grid-auto-rows`/`grid-auto-columns` — size one implicit track at a time and have no `repeat()` form.
    pub(crate) fn assert_single(&self) {
        if matches!(self.0, Track::Repeat(..)) {
            panic!(
                "grid_auto_rows/grid_auto_columns take single tracks; repeat() has no meaning as an auto track"
            )
        }
    }

    pub(crate) fn track_sizing_function(&self, surface: Size) -> TrackSizingFunction {
        match &self.0 {
            Track::Single(sizing) | Track::Repeat(_, sizing) => sizing.resolve(surface),
        }
    }

    pub(crate) fn template_component(&self, surface: Size) -> GridTemplateComponent<String> {
        match &self.0 {
            Track::Single(sizing) => GridTemplateComponent::Single(sizing.resolve(surface)),
            Track::Repeat(count, sizing) => GridTemplateComponent::Repeat(GridTemplateRepetition {
                count: *count,
                tracks: vec![sizing.resolve(surface)],
                line_names: Vec::new(),
            }),
        }
    }
}
