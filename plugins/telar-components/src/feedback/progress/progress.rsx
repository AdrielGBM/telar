[logic]
use crate::shared;
use telar::{ControlSize, NumericValue, SizeDimension};

/// A progress bar's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the bar paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgressTokens {
    /// Corner radius of the track and its fill. `None` rounds both ends fully, whatever the bar's thickness.
    pub radius: Option<f32>,
    /// The track's width when the bar names none, at the regular size, scaled with the bar's own. `None` is 16 ems of the document's text.
    pub width: Option<f32>,
    /// The track's thickness when the bar names none, at the regular size, scaled with the bar's own. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub height: Option<f32>,
}

impl ProgressTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The corners of a bar laid out at `rect`.
    pub fn radius(&self, rect: Rect) -> BorderRadius {
        self.radius
            .map_or_else(|| shared::pill(rect), BorderRadius::all)
    }

    /// The track's width in a bar of `size` whose call names none.
    pub fn width(&self, size: ControlSize) -> f32 {
        shared::width_or_measure(size, self.width, 16.0)
    }

    /// The track's thickness in a bar of `size` whose call names none.
    pub fn height(&self, size: ControlSize) -> f32 {
        self.height
            .map_or_else(|| shared::gap_md(size), |height| height * size.scale())
    }
}

/// A determinate bar filled to a bound value, from the inline start: the left edge on a left-to-right surface, the right edge on a right-to-left one.
///
/// A reader hears it as a progress bar standing at its reading out of 100, called what the call's `a11y_label` names it.
pub struct Props {
    /// Progress, normalized to 0.0..=1.0 (out-of-range inputs are clamped on read, not here).
    ///
    /// A closure rather than a signal, like [`color`](Self::color) beside it: a bar *reports* a reading, it never writes one, and a caller whose reading is derived from two services has no signal to hand over. Insisting on one is what makes a shell reimplement this widget next to the catalogue.
    #[props(into, default)]
    pub value: Reactive<f32>,
    /// How large the bar is drawn: its thickness and, unless it names one or stretches, its width, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// Fill accent. `None` (the default) is the theme accent.
    #[props(some, into, default)]
    pub color: Option<Reactive<Color>>,
    /// Track (rail) colour; `None` (the default) is the theme's muted token.
    #[props(some, into, default)]
    pub track_color: Option<Reactive<Color>>,
    /// Track width in px. `None` (the default) is [`ProgressTokens::width`]. Ignored under [`stretch`](Self::stretch).
    #[props(some, default)]
    pub width: Option<f32>,
    /// Fill the parent's width instead of taking a fixed one — what a bar inside a card wants, where a px track is either short of the card or past its edge.
    #[props(default)]
    pub stretch: bool,
    /// Track height in px. `None` (the default) is [`ProgressTokens::height`].
    #[props(some, default)]
    pub height: Option<f32>,
    /// Amends the paint of the track — its **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ProgressTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

fn track_width(stretch: bool, width: Option<f32>, size: ControlSize) -> SizeDimension {
    match stretch {
        true => SizeDimension::Percent(1.0),
        false => SizeDimension::Px(width.unwrap_or_else(|| ProgressTokens::current().width(size))),
    }
}

fn corner() -> f32 {
    ProgressTokens::current().radius.unwrap_or(BorderRadius::FULL)
}

let size = props.size;
let stretch = props.stretch;
let width = props.width;
let height = props.height;
let color = props.color.clone();
let track_color = props.track_color.clone();

[view]
box width:(track_width(stretch, width, size)) height:(height.unwrap_or_else(|| ProgressTokens::current().height(size))) fill:(shared::resolve(&track_color, shared::muted)) radius:(corner()) role:progressbar valued:(NumericValue::new(0.0..=100.0, f64::from($props.value.clamp(0.0, 1.0)) * 100.0).ok()) surface:$props.style
    // Overlays the track and is scaled from the edge the reading starts at, which is cheaper than laying a narrower box out on every update.
    box absolute:fill fill:(shared::resolve(&color, shared::accent)) radius:(corner()) scale_x:($props.value.clamp(0.0, 1.0)) transform_origin:start
