//! The canvas toolbar: the controls that write [`CanvasSettings`]. The environment sits in the top bar, and the size, zoom and rotation beside the canvas they act on, in its header.

use std::rc::Rc;

use telar::preview::{Matrices, PreviewEntry, ViewportPreset};
use telar::{
    Accessible, AlignItems, Children, ControlSize, Direction, LayoutError, LayoutItem, LayoutStyle,
    Reactive, RectStyle, Role, RwSignal, Size, Slots, StyledContainer, box_item, effect,
    registered_modes, signal,
};
use telar_components::{
    GroupProps, IconButtonProps, ItemProps, SelectProps, group, icon_button, item, select,
};
use telar_devtools::WORKBENCH_GRID;
use telar_icons::icon;

use crate::settings::{
    Background, CUSTOM_DEFAULT, CanvasSettings, CanvasSize, DEVICES, PRESETS, ZOOM_STEPS, Zoom,
};
use crate::state::WorkshopState;
use crate::strings::{
    self, BACKGROUND, BACKGROUND_DARK, BACKGROUND_LIGHT, BACKGROUND_PREVIEW,
    BACKGROUND_TRANSPARENT, CANVAS_SETTINGS, CONTROL_SIZE, CONTROL_SIZE_DEFAULT,
    CONTROL_SIZE_LARGE, CONTROL_SIZE_MINI, CONTROL_SIZE_REGULAR, CONTROL_SIZE_SMALL, DEVICE_FRAMES,
    DIRECTION, DIRECTION_AUTO, DIRECTION_LTR, DIRECTION_RTL, GRID, HIGH_CONTRAST, LOCALE,
    LOCALE_DEFAULT, MODE, MODE_DEFAULT, PROJECT_VIEWPORTS, REDUCED_MOTION, ROTATE, RULERS,
    VIEWPORT, VIEWPORT_CUSTOM, VIEWPORT_FILL, VIEWPORT_PRESETS, VIEWPORT_PREVIEW, ZOOM, ZOOM_FIT,
    ZOOM_PERCENT,
};

const NARROW: f32 = WORKBENCH_GRID * 11.0;
const REGULAR: f32 = WORKBENCH_GRID * 17.0;
const WIDE: f32 = WORKBENCH_GRID * 22.0;
/// The index a select shows as nothing chosen.
const NO_ROW: u32 = u32::MAX;

/// The top bar's settings: the mode, locale, direction, control size, background and high contrast every canvas is shown in, and reduced motion.
pub(crate) fn environment(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = state.canvas_settings();
    let modes = listed(registered_modes(), settings.peek().mode);
    let locales = listed(locales(state), settings.peek().locale);
    let items = vec![
        choice(
            MODE,
            REGULAR,
            optional(MODE_DEFAULT, modes),
            move || settings.with(|settings| settings.mode.clone()),
            move |mode| settings.update(|settings| settings.mode = mode),
        )?,
        choice(
            LOCALE,
            REGULAR,
            optional(LOCALE_DEFAULT, locales),
            move || settings.with(|settings| settings.locale.clone()),
            move |locale| settings.update(|settings| settings.locale = locale),
        )?,
        choice(
            DIRECTION,
            WIDE,
            vec![
                Row::choice(None, Label::Key(DIRECTION_AUTO)),
                Row::choice(Some(Direction::Ltr), Label::Key(DIRECTION_LTR)),
                Row::choice(Some(Direction::Rtl), Label::Key(DIRECTION_RTL)),
            ],
            move || settings.with(|settings| settings.direction),
            move |direction| settings.update(|settings| settings.direction = direction),
        )?,
        choice(
            CONTROL_SIZE,
            REGULAR,
            vec![
                Row::choice(None, Label::Key(CONTROL_SIZE_DEFAULT)),
                Row::choice(Some(ControlSize::Mini), Label::Key(CONTROL_SIZE_MINI)),
                Row::choice(Some(ControlSize::Small), Label::Key(CONTROL_SIZE_SMALL)),
                Row::choice(Some(ControlSize::Regular), Label::Key(CONTROL_SIZE_REGULAR)),
                Row::choice(Some(ControlSize::Large), Label::Key(CONTROL_SIZE_LARGE)),
            ],
            move || settings.with(|settings| settings.control_size),
            move |size| settings.update(|settings| settings.control_size = size),
        )?,
        choice(
            BACKGROUND,
            WIDE,
            vec![
                Row::choice(Background::Preview, Label::Key(BACKGROUND_PREVIEW)),
                Row::choice(Background::Transparent, Label::Key(BACKGROUND_TRANSPARENT)),
                Row::choice(Background::Light, Label::Key(BACKGROUND_LIGHT)),
                Row::choice(Background::Dark, Label::Key(BACKGROUND_DARK)),
            ],
            move || settings.with(|settings| settings.background),
            move |background| settings.update(|settings| settings.background = background),
        )?,
        reduced_motion(settings)?,
        high_contrast(settings)?,
    ];
    group_of(items)
}

/// The canvas header's settings for a framed preview: its size and its zoom.
pub(crate) fn frame_settings(
    state: &WorkshopState,
    entry: &PreviewEntry,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = state.canvas_settings();
    let asked = entry.env.viewport;
    let project = Matrices::installed().viewports();
    let items =
        vec![
            choice(
                VIEWPORT,
                WIDE,
                size_rows(asked, project),
                move || settings.with(|settings| SizeChoice::of(&settings.size)),
                move |choice| {
                    settings.update(|settings| choice.apply(settings, asked, project));
                },
            )?,
            choice(
                ZOOM,
                NARROW,
                std::iter::once(Row::choice(Zoom::Fit, Label::Key(ZOOM_FIT)))
                    .chain(ZOOM_STEPS.iter().map(|&percent| {
                        Row::choice(Zoom::Percent(percent), Label::Percent(percent))
                    }))
                    .collect(),
                move || settings.with(|settings| settings.zoom),
                move |zoom| settings.update(|settings| settings.zoom = zoom),
            )?,
        ];
    group_of(items)
}

/// Turns the canvas a quarter, swapping its width and height; off while it has no fixed size to turn. An item of a `toolbar`, so it is built among its children.
pub(crate) fn rotate(
    state: &WorkshopState,
    asked: Option<Size>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = state.canvas_settings();
    let project = Matrices::installed().viewports();
    icon_button(
        IconButtonProps::props()
            .icon(icon!("lucide:rotate-ccw-square"))
            .label(Reactive::of(|| strings::text(ROTATE)))
            .pressed(Reactive::of(move || {
                settings.with(|settings| settings.rotated)
            }))
            .disabled(Reactive::of(move || {
                settings.with(|settings| settings.upright_size(asked, project).is_none())
            }))
            .on_press(Rc::new(move || {
                settings.update(|settings| settings.rotated = !settings.rotated)
            }))
            .build(),
        Children::default(),
    )
}

/// Shows the grid over the canvas while pressed. An item of a `toolbar`, so it is built among its children.
pub(crate) fn grid(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = state.canvas_settings();
    toggle(
        icon!("lucide:grid-3x3"),
        GRID,
        move || settings.with(|settings| settings.grid),
        move || settings.update(|settings| settings.grid = !settings.grid),
    )
}

/// Shows the rulers along the stage while pressed. An item of a `toolbar`, so it is built among its children.
pub(crate) fn rulers(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = state.canvas_settings();
    toggle(
        icon!("lucide:ruler"),
        RULERS,
        move || settings.with(|settings| settings.rulers),
        move || settings.update(|settings| settings.rulers = !settings.rulers),
    )
}

fn toggle(
    icon: telar_icons::IconName,
    name: &'static str,
    pressed: impl Fn() -> bool + 'static,
    flip: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    icon_button(
        IconButtonProps::props()
            .icon(icon)
            .label(Reactive::of(move || strings::text(name)))
            .pressed(Reactive::of(pressed))
            .on_press(Rc::new(flip))
            .build(),
        Children::default(),
    )
}

fn reduced_motion(settings: RwSignal<CanvasSettings>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    toggle(
        icon!("lucide:snail"),
        REDUCED_MOTION,
        move || settings.with(|settings| settings.reduced_motion),
        move || settings.update(|settings| settings.reduced_motion = !settings.reduced_motion),
    )
}

/// Shows every canvas in high contrast while pressed; released, each canvas is left to its preview and the application.
fn high_contrast(settings: RwSignal<CanvasSettings>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    toggle(
        icon!("lucide:contrast"),
        HIGH_CONTRAST,
        move || settings.with(|settings| settings.high_contrast == Some(true)),
        move || {
            settings.update(|settings| {
                settings.high_contrast = match settings.high_contrast {
                    Some(true) => None,
                    _ => Some(true),
                }
            })
        },
    )
}

/// What a viewport select shows: the size settings name, with every custom size one choice.
#[derive(Clone, Debug, PartialEq)]
enum SizeChoice {
    Named(CanvasSize),
    Custom,
}

impl SizeChoice {
    fn of(size: &CanvasSize) -> Self {
        match size {
            CanvasSize::Custom { .. } => Self::Custom,
            named => Self::Named(named.clone()),
        }
    }

    /// Picks this size. Picking the custom one keeps a custom size as it is, and otherwise starts one at the size the canvas has, upright.
    fn apply(
        &self,
        settings: &mut CanvasSettings,
        asked: Option<Size>,
        project: &[ViewportPreset],
    ) {
        match self {
            Self::Named(size) => settings.size = size.clone(),
            Self::Custom if matches!(settings.size, CanvasSize::Custom { .. }) => {}
            Self::Custom => {
                let size = settings
                    .fixed_size(asked, project)
                    .unwrap_or(CUSTOM_DEFAULT);
                settings.size = CanvasSize::Custom {
                    width: size.width,
                    height: size.height,
                };
                settings.rotated = false;
            }
        }
    }
}

fn size_rows(asked: Option<Size>, project: &'static [ViewportPreset]) -> Vec<Row<SizeChoice>> {
    let named = |size: CanvasSize, label: Label, hint: Option<Size>| Row::Choice {
        value: SizeChoice::Named(size),
        label,
        hint: hint.map(dimensions),
    };
    let mut rows = vec![
        named(CanvasSize::Preview, Label::Key(VIEWPORT_PREVIEW), asked),
        named(CanvasSize::Fill, Label::Key(VIEWPORT_FILL), None),
        Row::Heading(Label::Key(VIEWPORT_PRESETS)),
    ];
    rows.extend(PRESETS.iter().map(|preset| {
        named(
            CanvasSize::Preset(preset.id.into()),
            Label::Key(preset.name),
            Some(preset.size),
        )
    }));
    if !project.is_empty() {
        rows.push(Row::Heading(Label::Key(PROJECT_VIEWPORTS)));
        rows.extend(project.iter().map(|preset| {
            named(
                CanvasSize::Project(preset.name.into()),
                Label::Text(preset.name.into()),
                Some(preset.size),
            )
        }));
    }
    rows.push(Row::Heading(Label::Key(DEVICE_FRAMES)));
    rows.extend(DEVICES.iter().map(|device| {
        named(
            CanvasSize::Device(device.id.into()),
            Label::Key(device.name),
            Some(device.size),
        )
    }));
    rows.push(Row::choice(SizeChoice::Custom, Label::Key(VIEWPORT_CUSTOM)));
    rows
}

fn dimensions(size: Size) -> String {
    format!("{} × {}", size.width.round(), size.height.round())
}

/// The locales a canvas can be shown in: the application catalog's, then any a preview asks for.
pub(crate) fn locales(state: &WorkshopState) -> Vec<String> {
    let catalog = telar::i18n::catalog().map_or(&[][..], |catalog| catalog.locales);
    let asked = state.entries().iter().filter_map(|entry| entry.env.locale);
    listed_all(catalog.iter().copied().chain(asked))
}

/// `options`, with `current` after them when it is not among them, so a setting restored from elsewhere still shows.
fn listed(options: Vec<String>, current: Option<String>) -> Vec<String> {
    listed_all(options.into_iter().chain(current))
}

fn listed_all<S: Into<String>>(options: impl IntoIterator<Item = S>) -> Vec<String> {
    let mut listed: Vec<String> = Vec::new();
    for option in options {
        let option = option.into();
        if !listed.contains(&option) {
            listed.push(option);
        }
    }
    listed
}

/// A row leaving the setting to the preview, then one per option.
fn optional(unset: &'static str, options: Vec<String>) -> Vec<Row<Option<String>>> {
    std::iter::once(Row::choice(None, Label::Key(unset)))
        .chain(
            options
                .into_iter()
                .map(|option| Row::choice(Some(option.clone()), Label::Text(option))),
        )
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
enum Label {
    /// A [`crate::strings`] key.
    Key(&'static str),
    Text(String),
    Percent(u16),
}

impl Label {
    fn text(&self) -> String {
        match self {
            Self::Key(key) => strings::text(key),
            Self::Text(text) => text.clone(),
            Self::Percent(percent) => strings::text_with(ZOOM_PERCENT, &percent.to_string()),
        }
    }

    fn reactive(&self) -> Reactive<String> {
        let label = self.clone();
        Reactive::of(move || label.text())
    }
}

/// One row of a settings select: a value it sets, or a heading over the values after it.
enum Row<T> {
    Choice {
        value: T,
        label: Label,
        /// Quiet text after the label, such as a size.
        hint: Option<String>,
    },
    Heading(Label),
}

impl<T: PartialEq> Row<T> {
    fn choice(value: T, label: Label) -> Self {
        Self::Choice {
            value,
            label,
            hint: None,
        }
    }

    fn holds(&self, held: &T) -> bool {
        matches!(self, Self::Choice { value, .. } if value == held)
    }

    fn build(&self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        match self {
            Self::Choice { label, hint, .. } => {
                let props = ItemProps::props().label(label.reactive());
                let props = match hint.clone() {
                    Some(hint) => props.hint(Reactive::of(move || hint.clone())),
                    None => props,
                };
                item(props.build(), Children::default())
            }
            Self::Heading(label) => group(
                GroupProps::props().label(label.reactive()).build(),
                Children::default(),
            ),
        }
    }
}

/// A select named `name`, `width` wide, showing the row holding what `current` reads and calling `pick` with the value of the row picked.
fn choice<T: Clone + PartialEq + 'static>(
    name: &'static str,
    width: f32,
    rows: Vec<Row<T>>,
    current: impl Fn() -> T + 'static,
    pick: impl Fn(T) + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let rows: Rc<[Row<T>]> = rows.into();
    let selected = signal(NO_ROW);
    let shown = Rc::clone(&rows);
    effect(move || {
        let current = current();
        let at = shown
            .iter()
            .position(|row| row.holds(&current))
            .map_or(NO_ROW, |at| at as u32);
        selected.set_if_changed(at);
    });
    let picked = Rc::clone(&rows);
    let on_select: Rc<dyn Fn(u32)> = Rc::new(move |at| {
        if let Some(Row::Choice { value, .. }) = picked.get(at as usize) {
            pick(value.clone());
        }
    });
    let field = select(
        SelectProps::props()
            .selected(selected)
            .stretch(true)
            .on_select(on_select)
            .build(),
        Children::new(move || {
            let mut slots = Slots::new();
            for row in rows.iter() {
                slots.push(None, row.build()?);
            }
            Ok(slots)
        }),
    )?
    .a11y_label(move || strings::text(name));
    let sized = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(width)
            .flex_shrink(0.0),
        |_| RectStyle::default(),
        vec![field],
    )?;
    Ok(box_item(sized))
}

fn group_of(items: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .gap(WORKBENCH_GRID),
        |_| RectStyle::default(),
        items,
    )?
    .role(Role::Group)
    .a11y_label(|| strings::text(CANVAS_SETTINGS));
    Ok(box_item(row))
}

#[cfg(test)]
#[path = "canvas_toolbar_test.rs"]
mod tests;
