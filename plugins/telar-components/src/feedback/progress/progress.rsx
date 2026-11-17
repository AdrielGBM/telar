[logic]
use crate::look::Look;
use crate::shared;
use telar::{ControlSize, NumericValue, SizeDimension, Tone, Variant};

/// A progress bar's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the bar paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProgressTokens {
    /// Corner radius of the track and its fill. `None` rounds both ends fully, whatever the bar's thickness.
    pub radius: Option<f32>,
    /// The width a bar takes where what holds it leaves the room, at the regular size, scaled with the bar's own: narrower where it does not, and as the call places it (`width:100%`). `None` is 16 ems of the document's text.
    pub width: Option<f32>,
    /// The track's thickness at the regular size, scaled with the bar's own. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub height: Option<f32>,
}

impl ProgressTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The corner radius of the track and its fill.
    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or(BorderRadius::FULL)
    }

    /// The width a bar of `size` takes where what holds it leaves the room.
    pub fn width(&self, size: ControlSize) -> f32 {
        shared::width_or_measure(size, self.width, 16.0)
    }

    /// The track's thickness in a bar of `size`.
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
    /// A closure rather than a signal: a bar *reports* a reading, it never writes one, and a caller whose reading is derived from two services has no signal to hand over. Insisting on one is what makes a shell reimplement this widget next to the catalogue.
    #[props(into, default, control = range(0, 1))]
    pub value: Reactive<f32>,
    /// How large the bar is drawn: its thickness and the width it takes where nothing narrower holds it, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
    /// What the bar's colour says: a named tone, or a colour of the caller's (`tone:$theme.brand`). The fill carries it whole and the track a wash of it, as a soft surface does.
    #[props(into, default)]
    pub tone: Reactive<Tone>,
    /// Amends the paint of the fill, the part of the track the reading has reached: `fill_style:@class` on the call.
    #[props(some, default)]
    pub fill_style: Option<SurfaceAmendment>,
    /// Amends the paint of the track — its **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ProgressTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

let size = props.size;
let reached = Look::shown(props.tone.clone(), Variant::Solid);
let rail = Look::shown(props.tone.clone(), Variant::Soft);

[view]
row width:(ProgressTokens::current().width(size)) max_width:100% min_width:0 height:(ProgressTokens::current().height(size)) fill:(rail.fill($text.ink)) radius:(ProgressTokens::current().radius()) role:progressbar valued:(NumericValue::new(0.0..=100.0, f64::from($props.value.clamp(0.0, 1.0)) * 100.0).ok()) surface:props.style clip
    box width:(SizeDimension::Percent($props.value.clamp(0.0, 1.0))) shrink:0 fill:(reached.fill($text.ink)) radius:(ProgressTokens::current().radius()) surface:props.fill_style
