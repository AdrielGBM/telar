//! The canvas toolbar: the controls that write [`CanvasSettings`]. The environment every preview is shown in is a menu in the top bar, and the size, zoom and rotation sit beside the canvas they act on, in its header.

use std::rc::Rc;

use telar::preview::{Matrices, PreviewEntry, ViewportPreset};
use telar::{
    Accessible, AlignItems, Children, Container, ControlSize, Direction, LayoutError, LayoutItem,
    LayoutStyle, Reactive, Role, RwSignal, Size, Slots, box_item, effect, signal,
};
use telar_components::{
    GroupProps, IconButtonProps, ItemProps, MenuProps, SelectProps, SeparatorProps, SubmenuProps,
    group, icon_button, item, menu, select, separator, submenu,
};
use telar_devtools::WORKBENCH_GRID;
use telar_icons::icon;

use crate::settings::{
    Background, CUSTOM_DEFAULT, CanvasSettings, CanvasSize, DEVICES, ModeChoice, PRESETS,
    ZOOM_STEPS, Zoom, mode_choices,
};
use crate::state::WorkshopState;
use crate::strings::{
    self, BACKGROUND, BACKGROUND_DARK, BACKGROUND_LIGHT, BACKGROUND_PREVIEW,
    BACKGROUND_TRANSPARENT, CONTROL_SIZE, CONTROL_SIZE_DEFAULT, CONTROL_SIZE_LARGE,
    CONTROL_SIZE_MINI, CONTROL_SIZE_REGULAR, CONTROL_SIZE_SMALL, DEVICE_FRAMES, DIRECTION,
    DIRECTION_AUTO, DIRECTION_LTR, DIRECTION_RTL, FRAME_SETTINGS, GRID, HIGH_CONTRAST, LOCALE,
    LOCALE_DEFAULT, MODE, PROJECT_VIEWPORTS, REDUCED_MOTION, ROTATE, RULERS, SETTINGS, VIEWPORT,
    VIEWPORT_CUSTOM, VIEWPORT_FILL, VIEWPORT_PRESETS, VIEWPORT_PREVIEW, ZOOM, ZOOM_FIT,
    ZOOM_PERCENT,
};

const NARROW: f32 = WORKBENCH_GRID * 11.0;
const REGULAR: f32 = WORKBENCH_GRID * 17.0;
const WIDE: f32 = WORKBENCH_GRID * 22.0;
/// The index a select shows as nothing chosen.
const NO_ROW: u32 = u32::MAX;

/// The top bar's preview settings as one menu: the mode, locale, direction, control size and background every canvas is shown in, each a row naming the one in use that opens a submenu of its choices, then reduced motion and high contrast, ticked while on.
pub(crate) fn environment_menu(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings: Rc<[Box<dyn Setting>]> = environment_settings(state).into();
    menu(
        MenuProps::props()
            .label(Reactive::of(|| strings::text(SETTINGS)))
            .caret(false)
            .fit(true)
            .build(),
        Children::new(move || {
            let mut slots = Slots::new();
            let mut after_choice = false;
            for setting in settings.iter() {
                if setting.is_switch() && after_choice {
                    slots.push(
                        None,
                        separator(SeparatorProps::props().build(), Children::default())?,
                    );
                }
                after_choice = !setting.is_switch();
                setting.menu_rows(&mut slots)?;
            }
            Ok(slots)
        }),
    )
}

fn environment_settings(state: &WorkshopState) -> Vec<Box<dyn Setting>> {
    let settings = state.canvas_settings();
    let modes = mode_choices();
    let unlisted = Some(settings.peek().mode).filter(|current| !modes.contains(current));
    let locales = listed(locales(state), settings.peek().locale);
    vec![
        Box::new(Choice::new(
            MODE,
            REGULAR,
            modes
                .into_iter()
                .chain(unlisted)
                .map(|mode| Row::choice(mode.clone(), Label::Mode(mode)))
                .collect(),
            move || settings.with(|settings| settings.mode.clone()),
            move |mode| settings.update(|settings| settings.mode = mode),
        )),
        Box::new(Choice::new(
            LOCALE,
            REGULAR,
            optional(LOCALE_DEFAULT, locales),
            move || settings.with(|settings| settings.locale.clone()),
            move |locale| settings.update(|settings| settings.locale = locale),
        )),
        Box::new(Choice::new(
            DIRECTION,
            WIDE,
            vec![
                Row::choice(None, Label::Key(DIRECTION_AUTO)),
                Row::choice(Some(Direction::Ltr), Label::Key(DIRECTION_LTR)),
                Row::choice(Some(Direction::Rtl), Label::Key(DIRECTION_RTL)),
            ],
            move || settings.with(|settings| settings.direction),
            move |direction| settings.update(|settings| settings.direction = direction),
        )),
        Box::new(Choice::new(
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
        )),
        Box::new(Choice::new(
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
        )),
        Box::new(reduced_motion(settings)),
        Box::new(high_contrast(settings)),
    ]
}

/// The canvas header's settings for a framed preview: its size and its zoom.
pub(crate) fn frame_settings(
    state: &WorkshopState,
    entry: &PreviewEntry,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = state.canvas_settings();
    let asked = entry.env.viewport;
    let project = Matrices::installed().viewports();
    let zooms = std::iter::once(Row::choice(Zoom::Fit, Label::Key(ZOOM_FIT)))
        .chain(
            ZOOM_STEPS
                .iter()
                .map(|&percent| Row::choice(Zoom::Percent(percent), Label::Percent(percent))),
        )
        .collect();
    let items = vec![
        Choice::new(
            VIEWPORT,
            WIDE,
            size_rows(asked, project),
            move || settings.with(|settings| SizeChoice::of(&settings.size)),
            move |choice| {
                settings.update(|settings| choice.apply(settings, asked, project));
            },
        )
        .inline()?,
        Choice::new(
            ZOOM,
            NARROW,
            zooms,
            move || settings.with(|settings| settings.zoom),
            move |zoom| settings.update(|settings| settings.zoom = zoom),
        )
        .inline()?,
    ];
    frame_group(items)
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
    Switch::new(
        icon!("lucide:grid-3x3"),
        GRID,
        move || settings.with(|settings| settings.grid),
        move || settings.update(|settings| settings.grid = !settings.grid),
    )
    .inline()
}

/// Shows the rulers along the stage while pressed. An item of a `toolbar`, so it is built among its children.
pub(crate) fn rulers(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = state.canvas_settings();
    Switch::new(
        icon!("lucide:ruler"),
        RULERS,
        move || settings.with(|settings| settings.rulers),
        move || settings.update(|settings| settings.rulers = !settings.rulers),
    )
    .inline()
}

fn reduced_motion(settings: RwSignal<CanvasSettings>) -> Switch {
    Switch::new(
        icon!("lucide:snail"),
        REDUCED_MOTION,
        move || settings.with(|settings| settings.reduced_motion),
        move || settings.update(|settings| settings.reduced_motion = !settings.reduced_motion),
    )
}

/// Shows every canvas in high contrast while on; off, each canvas is left to its preview and the application.
fn high_contrast(settings: RwSignal<CanvasSettings>) -> Switch {
    Switch::new(
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

/// One canvas setting, shown in place as a control of its own or as rows of a menu.
trait Setting {
    fn inline(&self) -> Result<Box<dyn LayoutItem>, LayoutError>;

    fn menu_rows(&self, slots: &mut Slots) -> Result<(), LayoutError>;

    fn is_switch(&self) -> bool {
        false
    }
}

/// A setting turned on and off: an icon button pressed while on, or a menu row with a switch.
struct Switch {
    icon: telar_icons::IconName,
    name: &'static str,
    on: Rc<dyn Fn() -> bool>,
    flip: Rc<dyn Fn()>,
}

impl Switch {
    fn new(
        icon: telar_icons::IconName,
        name: &'static str,
        on: impl Fn() -> bool + 'static,
        flip: impl Fn() + 'static,
    ) -> Self {
        Self {
            icon,
            name,
            on: Rc::new(on),
            flip: Rc::new(flip),
        }
    }

    fn label(&self) -> Reactive<String> {
        let name = self.name;
        Reactive::of(move || strings::text(name))
    }

    fn is_on(&self) -> Reactive<bool> {
        let on = Rc::clone(&self.on);
        Reactive::of(move || on())
    }
}

impl Setting for Switch {
    fn inline(&self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        icon_button(
            IconButtonProps::props()
                .icon(self.icon.clone())
                .label(self.label())
                .pressed(self.is_on())
                .on_press(Rc::clone(&self.flip))
                .build(),
            Children::default(),
        )
    }

    fn menu_rows(&self, slots: &mut Slots) -> Result<(), LayoutError> {
        let row = item(
            ItemProps::props()
                .label(self.label())
                .switch(self.is_on())
                .on_press(Rc::clone(&self.flip))
                .build(),
            Children::default(),
        )?;
        slots.push(None, row);
        Ok(())
    }

    fn is_switch(&self) -> bool {
        true
    }
}

/// A setting that takes one of `rows`' values: a select named `name` and `width` wide, or a menu row named `name` and showing the value `current` reads, which opens a submenu of a row per value with that one ticked. Either calls `pick` with the value of the row picked.
struct Choice<T> {
    name: &'static str,
    width: f32,
    rows: Rc<[Row<T>]>,
    current: Rc<dyn Fn() -> T>,
    pick: Rc<dyn Fn(T)>,
}

impl<T: Clone + PartialEq + 'static> Choice<T> {
    fn new(
        name: &'static str,
        width: f32,
        rows: Vec<Row<T>>,
        current: impl Fn() -> T + 'static,
        pick: impl Fn(T) + 'static,
    ) -> Self {
        Self {
            name,
            width,
            rows: rows.into(),
            current: Rc::new(current),
            pick: Rc::new(pick),
        }
    }

    /// What the setting holds now, in the words of its row: the hint its submenu's row shows.
    fn shown(&self) -> Reactive<String> {
        let (rows, current) = (Rc::clone(&self.rows), Rc::clone(&self.current));
        Reactive::of(move || {
            let current = current();
            rows.iter()
                .find_map(|row| match row {
                    Row::Choice { value, label, .. } if *value == current => Some(label.text()),
                    _ => None,
                })
                .unwrap_or_default()
        })
    }

    /// The rows of its submenu: one per value, the one it holds ticked, and the headings between them.
    fn choices(&self) -> Children {
        let (rows, current, pick) = (
            Rc::clone(&self.rows),
            Rc::clone(&self.current),
            Rc::clone(&self.pick),
        );
        Children::new(move || {
            let mut slots = Slots::new();
            for row in rows.iter() {
                slots.push(None, choice_row(row, &current, &pick)?);
            }
            Ok(slots)
        })
    }
}

/// One value of a setting as a row of its submenu, one of a set with the value held ticked; a heading as a heading.
fn choice_row<T: Clone + PartialEq + 'static>(
    row: &Row<T>,
    current: &Rc<dyn Fn() -> T>,
    pick: &Rc<dyn Fn(T)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Row::Choice { value, label, hint } = row else {
        return row.build();
    };
    let (held, current) = (value.clone(), Rc::clone(current));
    let (value, pick) = (value.clone(), Rc::clone(pick));
    let props = ItemProps::props()
        .label(label.reactive())
        .checked(Reactive::of(move || current() == held))
        .radio(true)
        .on_press(Rc::new(move || pick(value.clone())));
    let props = match hint.clone() {
        Some(hint) => props.hint(Reactive::of(move || hint.clone())),
        None => props,
    };
    item(props.build(), Children::default())
}

impl<T: Clone + PartialEq + 'static> Setting for Choice<T> {
    fn inline(&self) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let selected = signal(NO_ROW);
        let (shown, current) = (Rc::clone(&self.rows), Rc::clone(&self.current));
        effect(move || {
            let current = current();
            let at = shown
                .iter()
                .position(|row| row.holds(&current))
                .map_or(NO_ROW, |at| at as u32);
            selected.set_if_changed(at);
        });
        let (picked, pick) = (Rc::clone(&self.rows), Rc::clone(&self.pick));
        let on_select: Rc<dyn Fn(u32)> = Rc::new(move |at| {
            if let Some(Row::Choice { value, .. }) = picked.get(at as usize) {
                pick(value.clone());
            }
        });
        let (rows, name) = (Rc::clone(&self.rows), self.name);
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
        // A row, so the stretched select grows along the width it is given: in a column it grew along the height instead, leaving this box no height and the select spilling out below it.
        let sized = Container::new(
            LayoutStyle::row().width(self.width).flex_shrink(0.0),
            vec![field],
        )?;
        Ok(box_item(sized))
    }

    fn menu_rows(&self, slots: &mut Slots) -> Result<(), LayoutError> {
        let name = self.name;
        let row = submenu(
            SubmenuProps::props()
                .label(Reactive::of(move || strings::text(name)))
                .hint(self.shown())
                .build(),
            self.choices(),
        )?;
        slots.push(None, row);
        Ok(())
    }
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
    Mode(ModeChoice),
}

impl Label {
    fn text(&self) -> String {
        match self {
            Self::Key(key) => strings::text(key),
            Self::Text(text) => text.clone(),
            Self::Percent(percent) => strings::text_with(ZOOM_PERCENT, &percent.to_string()),
            Self::Mode(mode) => mode.label(),
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

fn frame_group(items: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let row = Container::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .gap(WORKBENCH_GRID),
        items,
    )?
    .role(Role::Group)
    .a11y_label(|| strings::text(FRAME_SETTINGS));
    Ok(box_item(row))
}

#[cfg(test)]
#[path = "canvas_toolbar_test.rs"]
mod tests;
