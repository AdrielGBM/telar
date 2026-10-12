//! [`CanvasSettings`]: what the canvas toolbar sets, and how it combines with what a preview asks for into the environment its canvas gets.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use telar::preview::{PreviewEnv, ViewportPreset};
use telar::{
    Color, ColorScheme, ControlSize, Direction, Insets, Size, registered_modes, scheme_mode,
};

use crate::strings::{
    self, DEVICE_COMPACT_PHONE, DEVICE_PHONE, DEVICE_TABLET, MODE_DARK, MODE_LIGHT, MODE_SYSTEM,
    PRESET_DESKTOP, PRESET_LAPTOP, PRESET_MOBILE, PRESET_TABLET,
};

/// The size a custom viewport starts at when nothing gives it one.
pub(crate) const CUSTOM_DEFAULT: Size = Size::new(800.0, 600.0);
pub(crate) const LIGHT_BACKGROUND: Color = Color::WHITE;
pub(crate) const DARK_BACKGROUND: Color = Color::rgb(0.07, 0.07, 0.08);
pub(crate) const ZOOM_STEPS: &[u16] = &[25, 50, 75, 100, 150, 200];
/// What [`Zoom::Fit`] steps in and out from, not knowing the scale it fits at.
const ACTUAL_SIZE: u16 = 100;
/// The grid's spacing, in the canvas's logical px.
pub(crate) const GRID_STEP: f32 = 8.0;

/// Everything the canvas toolbar sets, for every preview alike. One value, so it is kept, restored and stored whole.
///
/// A field at its default leaves the decision to the preview's own [`PreviewEnv`], and where that names nothing, to the application: for the mode, to the application's mode for the system's scheme first ([`ModeChoice::System`]).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CanvasSettings {
    pub(crate) mode: ModeChoice,
    /// A BCP 47 language tag.
    pub(crate) locale: Option<String>,
    /// Set only by choosing one; otherwise the canvas reads in the direction of its locale.
    #[serde(with = "direction_text")]
    pub(crate) direction: Option<Direction>,
    #[serde(with = "control_size_text")]
    pub(crate) control_size: Option<ControlSize>,
    pub(crate) background: Background,
    pub(crate) size: CanvasSize,
    /// Swaps the width and height of whatever fixed size the canvas has, and turns a device's safe area with it.
    pub(crate) rotated: bool,
    pub(crate) zoom: Zoom,
    /// Process-wide: motion follows one override for the whole application.
    pub(crate) reduced_motion: bool,
    /// Per canvas, unlike reduced motion: the chrome around it keeps the application's contrast.
    pub(crate) high_contrast: Option<bool>,
    /// Lines over the canvas every [`GRID_STEP`] of its logical px. The workshop's own: the preview never sees them.
    pub(crate) grid: bool,
    /// Rulers along the top and leading edges of the stage, measuring the canvas in its logical px.
    pub(crate) rulers: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Background {
    /// The preview's own, or the application theme's surface.
    #[default]
    Preview,
    /// Shows the checker through.
    Transparent,
    Light,
    Dark,
}

impl Background {
    /// The canvas background this choice gives a preview asking for `asked`.
    pub(crate) fn color(self, asked: Option<Color>) -> Option<Color> {
        match self {
            Self::Preview => asked,
            Self::Transparent => Some(Color::TRANSPARENT),
            Self::Light => Some(LIGHT_BACKGROUND),
            Self::Dark => Some(DARK_BACKGROUND),
        }
    }
}

/// The mode the canvas is shown in. Stored and carried as one word ([`Self::word`]): `system`, `light`, `dark`, or a mode's id. The words `light` and `dark` are the scheme choices, resolving to the application's light and dark modes, and none of the three names a mode: one registered as `light` or `dark` is reached through the scheme it declares.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum ModeChoice {
    /// The preview's own mode, else the application's mode for the scheme the workshop is in, following the system as it changes.
    #[default]
    System,
    /// The application's light mode ([`scheme_mode`]).
    Light,
    /// The application's dark mode ([`scheme_mode`]).
    Dark,
    /// A mode registered with `register_mode`, by id.
    Named(String),
}

impl ModeChoice {
    pub(crate) fn word(&self) -> &str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Named(id) => id,
        }
    }

    /// The choice that shows the mode `id`: the scheme choice where `id` is the application's mode for that scheme, so the menu ticks it and it is kept as it was picked.
    pub(crate) fn for_mode(id: &str) -> Self {
        let stands_for = |scheme| scheme_mode(scheme).as_deref() == Some(id);
        if stands_for(ColorScheme::Light) {
            Self::Light
        } else if stands_for(ColorScheme::Dark) {
            Self::Dark
        } else {
            Self::Named(id.to_string())
        }
    }

    /// The choice [`Self::word`] wrote; an empty word is no choice made.
    pub(crate) fn parse(word: &str) -> Self {
        match word {
            "" | "system" => Self::System,
            "light" => Self::Light,
            "dark" => Self::Dark,
            id => Self::Named(id.to_string()),
        }
    }

    pub(crate) fn label(&self) -> String {
        match self {
            Self::System => strings::text(MODE_SYSTEM),
            Self::Light => strings::text(MODE_LIGHT),
            Self::Dark => strings::text(MODE_DARK),
            Self::Named(id) => id.clone(),
        }
    }

    /// The mode a canvas is shown in for a preview asking for `asked`, while the workshop is in `scheme`. A scheme the application has no mode for leaves the preview its own, as a setting that names nothing does.
    pub(crate) fn mode(&self, asked: Option<&str>, scheme: ColorScheme) -> Option<String> {
        let asked = || asked.map(str::to_string);
        match self {
            Self::System => asked().or_else(|| scheme_mode(scheme)),
            Self::Light => scheme_mode(ColorScheme::Light).or_else(asked),
            Self::Dark => scheme_mode(ColorScheme::Dark).or_else(asked),
            Self::Named(id) => Some(id.clone()),
        }
    }
}

impl Serialize for ModeChoice {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.word().serialize(serializer)
    }
}

/// `null`, what a workshop that offered no system choice stored for its default, reads as [`ModeChoice::System`].
impl<'de> Deserialize<'de> for ModeChoice {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let word = Option::<String>::deserialize(deserializer)?;
        Ok(word.as_deref().map_or(Self::System, Self::parse))
    }
}

/// What the Mode setting offers: following the system, the application's light and dark modes where it has them, then each other registered mode by id, leaving out one named with a word [`ModeChoice::parse`] reads as another choice.
pub(crate) fn mode_choices() -> Vec<ModeChoice> {
    let light = scheme_mode(ColorScheme::Light);
    let dark = scheme_mode(ColorScheme::Dark);
    let mut choices = vec![ModeChoice::System];
    if light.is_some() {
        choices.push(ModeChoice::Light);
    }
    if dark.is_some() {
        choices.push(ModeChoice::Dark);
    }
    choices.extend(
        registered_modes()
            .into_iter()
            .filter(|id| Some(id) != light.as_ref() && Some(id) != dark.as_ref())
            .map(|id| ModeChoice::parse(&id))
            .filter(|choice| matches!(choice, ModeChoice::Named(_))),
    );
    choices
}

/// How big the canvas is.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CanvasSize {
    /// The size the preview asks for, or else filling the stage.
    #[default]
    Preview,
    /// Filling the stage, whatever the preview asks for.
    Fill,
    /// One of [`PRESETS`], by id.
    Preset(String),
    /// One of the project's `[telar.previews.viewports]`, by name.
    Project(String),
    /// One of [`DEVICES`], by id: its size, its safe area and a bezel around it.
    Device(String),
    /// What the resize handles set, in logical px.
    Custom { width: f32, height: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Zoom {
    /// Actual size, or smaller where that is what it takes for the whole canvas to show.
    #[default]
    Fit,
    Percent(u16),
}

impl Zoom {
    /// The scale a fixed zoom shows the canvas at; `None` to fit.
    pub(crate) fn fixed_scale(self) -> Option<f32> {
        match self {
            Self::Fit => None,
            Self::Percent(percent) => Some(f32::from(percent) / 100.0),
        }
    }

    /// The next of [`ZOOM_STEPS`] above this zoom, or the largest.
    pub(crate) fn zoomed_in(self) -> Self {
        let now = self.percent();
        let next = ZOOM_STEPS.iter().copied().find(|&step| step > now);
        Self::Percent(next.unwrap_or(ZOOM_STEPS[ZOOM_STEPS.len() - 1]))
    }

    /// The next of [`ZOOM_STEPS`] below this zoom, or the smallest.
    pub(crate) fn zoomed_out(self) -> Self {
        let now = self.percent();
        let next = ZOOM_STEPS.iter().rev().copied().find(|&step| step < now);
        Self::Percent(next.unwrap_or(ZOOM_STEPS[0]))
    }

    fn percent(self) -> u16 {
        match self {
            Self::Fit => ACTUAL_SIZE,
            Self::Percent(percent) => percent,
        }
    }
}

/// A viewport size the toolbar offers, drawn with no frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Preset {
    pub(crate) id: &'static str,
    /// A [`crate::strings`] key.
    pub(crate) name: &'static str,
    pub(crate) size: Size,
}

/// [`PRESETS`]' sizes as a matrix's viewport axis writes them, in the same order.
pub(crate) const PRESET_VIEWPORTS: &[&str] = &["360x800", "768x1024", "1280x800", "1920x1080"];

pub(crate) const PRESETS: &[Preset] = &[
    Preset {
        id: "mobile",
        name: PRESET_MOBILE,
        size: Size::new(360.0, 800.0),
    },
    Preset {
        id: "tablet",
        name: PRESET_TABLET,
        size: Size::new(768.0, 1024.0),
    },
    Preset {
        id: "laptop",
        name: PRESET_LAPTOP,
        size: Size::new(1280.0, 800.0),
    },
    Preset {
        id: "desktop",
        name: PRESET_DESKTOP,
        size: Size::new(1920.0, 1080.0),
    },
];

/// A device the canvas can be shown as: a size, the edges its system keeps for itself, and a bezel. Upright; [`CanvasSettings::rotated`] turns it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Device {
    pub(crate) id: &'static str,
    /// A [`crate::strings`] key.
    pub(crate) name: &'static str,
    pub(crate) size: Size,
    pub(crate) safe_area: Insets,
}

pub(crate) const DEVICES: &[Device] = &[
    Device {
        id: "phone",
        name: DEVICE_PHONE,
        size: Size::new(390.0, 844.0),
        safe_area: Insets::new(47.0, 0.0, 34.0, 0.0),
    },
    Device {
        id: "compact-phone",
        name: DEVICE_COMPACT_PHONE,
        size: Size::new(375.0, 667.0),
        safe_area: Insets::new(20.0, 0.0, 0.0, 0.0),
    },
    Device {
        id: "tablet",
        name: DEVICE_TABLET,
        size: Size::new(820.0, 1180.0),
        safe_area: Insets::new(24.0, 0.0, 20.0, 0.0),
    },
];

impl CanvasSettings {
    /// The device the canvas is shown as, if it is one.
    pub(crate) fn device(&self) -> Option<&'static Device> {
        match &self.size {
            CanvasSize::Device(id) => DEVICES.iter().find(|device| device.id == id),
            _ => None,
        }
    }

    /// The fixed size [`Self::size`] names, before rotation: `None` for a canvas that fills the stage, and for a name nothing answers to any more.
    pub(crate) fn upright_size(
        &self,
        asked: Option<Size>,
        project: &[ViewportPreset],
    ) -> Option<Size> {
        match &self.size {
            CanvasSize::Preview => asked,
            CanvasSize::Fill => None,
            CanvasSize::Preset(id) => PRESETS
                .iter()
                .find(|preset| preset.id == id)
                .map(|preset| preset.size),
            CanvasSize::Project(name) => project
                .iter()
                .find(|preset| preset.name == name)
                .map(|preset| preset.size),
            CanvasSize::Device(_) => self.device().map(|device| device.size),
            CanvasSize::Custom { width, height } => Some(Size::new(*width, *height)),
        }
    }

    /// [`Self::upright_size`], turned when [`Self::rotated`].
    pub(crate) fn fixed_size(
        &self,
        asked: Option<Size>,
        project: &[ViewportPreset],
    ) -> Option<Size> {
        self.upright_size(asked, project)
            .map(|size| if self.rotated { rotate(size) } else { size })
    }

    /// The canvas's environment: these settings over `env`, the preview's own, while the workshop is in `scheme`. `framed` is false for a fullscreen preview, which takes no size, device or safe area.
    pub(crate) fn resolve(
        &self,
        env: &PreviewEnv,
        framed: bool,
        project: &[ViewportPreset],
        scheme: ColorScheme,
    ) -> CanvasEnv {
        let locale = self
            .locale
            .clone()
            .or_else(|| env.locale.map(str::to_string));
        // A canvas's locale does not turn its direction the way the application's does, so the canvas follows its own here.
        let direction = self
            .direction
            .or(env.direction)
            .or_else(|| locale.as_deref().map(Direction::for_locale));
        let safe_area = self
            .device()
            .filter(|_| framed)
            .map_or_else(Insets::default, |device| {
                if self.rotated {
                    rotate_insets(device.safe_area)
                } else {
                    device.safe_area
                }
            });
        CanvasEnv {
            mode: self.mode.mode(env.mode, scheme),
            locale,
            direction,
            control_size: self.control_size,
            high_contrast: self.high_contrast.or(env.high_contrast),
            background: self.background.color(env.background),
            viewport: self.fixed_size(env.viewport, project).filter(|_| framed),
            safe_area,
        }
    }
}

/// What one canvas is given: [`CanvasSettings::resolve`]'s answer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CanvasEnv {
    pub(crate) mode: Option<String>,
    pub(crate) locale: Option<String>,
    pub(crate) direction: Option<Direction>,
    pub(crate) control_size: Option<ControlSize>,
    pub(crate) high_contrast: Option<bool>,
    pub(crate) background: Option<Color>,
    pub(crate) viewport: Option<Size>,
    pub(crate) safe_area: Insets,
}

/// The scale that shows a `viewport` canvas whole in `room`, never above actual size.
pub(crate) fn fit_scale(viewport: Size, room: Size) -> f32 {
    let fits = (room.width / viewport.width).min(room.height / viewport.height);
    if fits.is_finite() && fits > 0.0 {
        fits.min(1.0)
    } else {
        1.0
    }
}

fn rotate(size: Size) -> Size {
    Size::new(size.height, size.width)
}

/// An upright device's safe area once it is turned a quarter anticlockwise, its top edge now on the left.
fn rotate_insets(insets: Insets) -> Insets {
    Insets::new(insets.right, insets.bottom, insets.left, insets.top)
}

macro_rules! word_text {
    ($module:ident, $type:ty, $word:ident, $parse:ident) => {
        mod $module {
            use serde::{Deserialize, Deserializer, Serialize, Serializer};
            use telar::preview::{$parse, $word};

            use super::*;

            pub(super) fn serialize<S: Serializer>(
                value: &Option<$type>,
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                value.map($word).serialize(serializer)
            }

            pub(super) fn deserialize<'de, D: Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Option<$type>, D::Error> {
                let word = Option::<String>::deserialize(deserializer)?;
                Ok(word.as_deref().and_then($parse))
            }
        }
    };
}

word_text!(direction_text, Direction, direction_word, parse_direction);
word_text!(
    control_size_text,
    ControlSize,
    control_size_word,
    parse_control_size
);

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
