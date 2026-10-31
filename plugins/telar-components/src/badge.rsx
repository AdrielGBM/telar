[logic]
/// A badge's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role named beside it, read when the badge paints and lays out, so a theme that moves the role still moves every badge it has not given a value of its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BadgeTokens {
    /// Corner radius. `None` is a pill: fully rounded at whatever height the badge is laid out.
    pub radius: Option<f32>,
    /// Space between the edge and the label at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_md`](crate::metrics::gap_md).
    pub pad_x: Option<f32>,
}

impl BadgeTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    /// The corner radius, [`BorderRadius::FULL`] for a pill.
    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or(BorderRadius::FULL)
    }

    /// The space between the edge and the label at the control size in force.
    pub fn pad_x(&self) -> f32 {
        self.pad_x
            .map_or_else(crate::shared::gap_md, |pad| pad * telar::control_scale())
    }
}

/// A small solid pill tag: an accent-filled box with a short label, at a caption's share of the text around it, in whichever ink reads on that fill. Non-interactive, unlike `button`.
pub struct Props {
    /// The words on the badge.
    #[props(into, default)]
    pub label: Reactive<String>,
    /// Fill colour. `Color::TRANSPARENT` (the default) means "unset" -> the theme's `primary()`. A closure (re-read every frame) so a theme token or `$signal` colour re-colours live, like `button`'s `fill`.
    #[props(into, default = Reactive::of(|| Color::TRANSPARENT))]
    pub color: Reactive<Color>,
    /// Amends the paint of the pill — the badge's **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`BadgeTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

[view]
row align:center justify:center pad_x:(BadgeTokens::current().pad_x()) pad_y:(crate::shared::gap_xs()) fill:(crate::shared::resolve(&props.color, crate::shared::accent)) radius:(BadgeTokens::current().radius()) surface:$props.style
    text "{$props.label}" font_size:(telar::control_scale() * crate::shared::CAPTION_RATIO)em color:(crate::shared::ink_on(crate::shared::resolve(&props.color, crate::shared::accent)))
