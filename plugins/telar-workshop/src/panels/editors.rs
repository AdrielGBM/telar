//! The editor each arg gets, chosen by its control, and the two-way binding between what an editor shows and what its arg holds.

use std::cell::RefCell;
use std::rc::Rc;

use telar::preview::host::{ArgState, Args};
use telar::preview::{ArgValue, ControlKind, NumberControl};
use telar::{
    Accessible, AlignItems, Border, BorderRadius, Children, Color, LayoutError, LayoutItem,
    LayoutStyle, Reactive, ReadSignal, RectStyle, RwSignal, ShapeStyle, Slots, StyledContainer,
    Text, TextArea, box_item, effect, signal,
};
use telar_components::{
    ItemProps, ScrubFieldProps, SelectProps, SliderProps, TabsProps, TextFieldProps, ToggleProps,
    item, scrub_field, select, slider, tabs, text_field, toggle,
};
use telar_devtools::{WORKBENCH_GRID, WORKBENCH_RADIUS, use_workbench_tokens};

use crate::strings::{self, UNSET, UNSET_ARG};

const FIELD_WIDTH: f32 = 220.0;
const SLIDER_WIDTH: f32 = 160.0;
const SWATCH_SIZE: f32 = 20.0;
const MULTILINE_HEIGHT: f32 = 64.0;

type Current = Rc<dyn Fn() -> Option<ArgValue>>;

/// One arg as its row reaches it: the args to set it on, and the row's view of what it holds now.
#[derive(Clone)]
pub(super) struct Binding {
    pub(super) args: Args,
    pub(super) name: &'static str,
    pub(super) state: ReadSignal<ArgState>,
}

/// An editor, and the value it would set its arg to now.
pub(super) struct Editor {
    pub(super) item: Box<dyn LayoutItem>,
    current: Current,
}

struct Bound<W: 'static> {
    shown: RwSignal<W>,
    current: Current,
}

impl Binding {
    /// A signal for an editor to show and edit, kept in step with the arg both ways: an edit sets the arg, and the arg moving for any other reason — a reset, the preview writing a live arg — moves the editor.
    ///
    /// `read` answers `None` for a value the editor cannot show, and `write` for one it cannot set yet, such as a colour half typed. Only edits are written back, so an editor that shows a value inexactly, like an `f64` on an `f32` slider, never marks its arg edited by being shown.
    fn bind<W: Clone + PartialEq + 'static>(
        &self,
        fallback: W,
        read: impl Fn(&ArgValue) -> Option<W> + 'static,
        write: impl Fn(&W) -> Option<ArgValue> + 'static,
    ) -> Bound<W> {
        let initial = self
            .state
            .peek()
            .value
            .and_then(|value| read(&value))
            .unwrap_or(fallback);
        let shown = signal(initial.clone());
        let synced = Rc::new(RefCell::new(initial));
        let write = Rc::new(write);
        let show = {
            let synced = Rc::clone(&synced);
            Rc::new(move |value: &ArgValue| {
                if let Some(next) = read(value)
                    && next != shown.peek()
                {
                    *synced.borrow_mut() = next.clone();
                    shown.set(next);
                }
            })
        };
        {
            let (state, write, show) = (self.state, Rc::clone(&write), Rc::clone(&show));
            effect(move || {
                let Some(value) = state.with(|arg| arg.value.clone()) else {
                    return;
                };
                if write(&shown.peek()).as_ref() != Some(&value) {
                    show(&value);
                }
            });
        }
        {
            let (binding, write) = (self.clone(), Rc::clone(&write));
            effect(move || {
                let edited = shown.get();
                if synced.replace(edited.clone()) == edited {
                    return;
                }
                let Some(value) = write(&edited) else {
                    return;
                };
                let held = binding.state.peek().value;
                if held.as_ref() == Some(&value) {
                    return;
                }
                if binding.args.set(binding.name, value).is_err()
                    && let Some(held) = held
                {
                    show(&held);
                }
            });
        }
        Bound {
            shown,
            current: Rc::new(move || write(&shown.peek())),
        }
    }

    fn value_text(&self) -> String {
        self.state.with(|arg| {
            arg.value
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default()
        })
    }
}

/// The editor `control` calls for, named by the arg it edits.
pub(super) fn editor(binding: &Binding, control: ControlKind) -> Result<Editor, LayoutError> {
    match control {
        ControlKind::Toggle => switch(binding),
        ControlKind::Number(number) => number_editor(binding, number),
        ControlKind::Text { multiline } => text_editor(binding, multiline),
        ControlKind::Color => color_editor(binding),
        ControlKind::Choice { variants } => {
            choice_editor(binding, variants, control.is_segmented())
        }
        ControlKind::Optional(inner) => optional_editor(binding, *inner),
        _ => read_only(binding),
    }
}

fn switch(binding: &Binding) -> Result<Editor, LayoutError> {
    let bound = binding.bind(
        false,
        |value| match value {
            ArgValue::Bool(on) => Some(*on),
            _ => None,
        },
        |on| Some(ArgValue::Bool(*on)),
    );
    let name = binding.name;
    let item = toggle(
        ToggleProps::props().checked(bound.shown).build(),
        Children::default(),
    )?
    .a11y_label(move || name);
    Ok(Editor {
        item,
        current: bound.current,
    })
}

fn number_editor(binding: &Binding, number: NumberControl) -> Result<Editor, LayoutError> {
    let fallback = number.range.map_or(0.0, |(min, _)| min as f32);
    let bound = binding.bind(fallback, number_of, move |value| {
        number_value(number, *value)
    });
    let name = binding.name;
    let item = match number.range {
        Some((min, max)) => {
            let step = number
                .step
                .map_or(if number.integer { 1.0 } else { 0.0 }, |step| step as f32);
            let slider = slider(
                SliderProps::props()
                    .value(bound.shown)
                    .min(min as f32)
                    .max(max as f32)
                    .step(step)
                    .width(SLIDER_WIDTH)
                    .build(),
                Children::default(),
            )?
            .a11y_label(move || name);
            let shown = binding.clone();
            let value = Text::declaring(move || shown.value_text(), LayoutStyle::new(), mono)?;
            inline(vec![slider, box_item(value)])?
        }
        None => {
            let props = ScrubFieldProps::props()
                .value(bound.shown)
                .step(number.step.map_or(1.0, |step| step as f32));
            let props = match number.integer {
                true => props.parse(Rc::new(|text: &str| {
                    text.trim()
                        .parse::<f32>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .map(f32::round)
                })),
                false => props,
            };
            scrub_field(props.build(), Children::default())?.a11y_label(move || name)
        }
    };
    Ok(Editor {
        item,
        current: bound.current,
    })
}

fn number_of(value: &ArgValue) -> Option<f32> {
    match *value {
        ArgValue::Int(value) => Some(value as f32),
        ArgValue::Float(value) => Some(value as f32),
        _ => None,
    }
}

fn number_value(number: NumberControl, value: f32) -> Option<ArgValue> {
    if !value.is_finite() {
        return None;
    }
    if number.integer {
        return Some(ArgValue::Int(value.round() as i128));
    }
    // Through the shortest text that reads back as the same `f32`, so a slider at 0.1 sets 0.1 rather than 0.10000000149011612.
    value.to_string().parse().ok().map(ArgValue::Float)
}

fn text_editor(binding: &Binding, multiline: bool) -> Result<Editor, LayoutError> {
    let bound = binding.bind(
        String::new(),
        |value| match value {
            ArgValue::Text(text) => Some(text.clone()),
            _ => None,
        },
        |text| Some(ArgValue::Text(text.clone())),
    );
    let name = binding.name;
    let item = match multiline {
        false => text_field(
            TextFieldProps::props()
                .value(bound.shown)
                .placeholder(name)
                .width(FIELD_WIDTH)
                .build(),
            Children::default(),
        )?,
        true => {
            let area = TextArea::declaring(bound.shown, LayoutStyle::new(), |text| text)?
                .a11y_label(move || name);
            let framed = StyledContainer::new(
                LayoutStyle::new()
                    .flex_column()
                    .width(FIELD_WIDTH)
                    .min_height(MULTILINE_HEIGHT)
                    .padding_horizontal(WORKBENCH_GRID)
                    .padding_vertical(WORKBENCH_GRID / 2.0),
                |_| field_frame(),
                vec![box_item(area)],
            )?;
            box_item(framed)
        }
    };
    Ok(Editor {
        item,
        current: bound.current,
    })
}

fn color_editor(binding: &Binding) -> Result<Editor, LayoutError> {
    let bound = binding.bind(
        String::new(),
        |value| matches!(value, ArgValue::Color(_)).then(|| value.to_string()),
        |text| color_value(text),
    );
    let name = binding.name;
    let state = binding.state;
    let swatch = StyledContainer::new(
        LayoutStyle::new()
            .width(SWATCH_SIZE)
            .height(SWATCH_SIZE)
            .flex_shrink(0.0),
        move |_| {
            let color = match state.with(|arg| arg.value.clone()) {
                Some(ArgValue::Color(color)) => color,
                _ => Color::TRANSPARENT,
            };
            RectStyle::default()
                .with_fill(color)
                .with_border(Border::uniform(use_workbench_tokens().border_subtle, 1.0))
                .with_radius(BorderRadius::all(WORKBENCH_RADIUS))
        },
        Vec::new(),
    )?
    .a11y_hidden();
    let field = text_field(
        TextFieldProps::props()
            .value(bound.shown)
            .placeholder(name)
            .width(FIELD_WIDTH - SWATCH_SIZE - WORKBENCH_GRID)
            .build(),
        Children::default(),
    )?;
    Ok(Editor {
        item: inline(vec![box_item(swatch), field])?,
        current: bound.current,
    })
}

/// A colour in any form an arg's text takes, with or without its leading `#`.
fn color_value(text: &str) -> Option<ArgValue> {
    let text = text.trim();
    let value: ArgValue = text
        .parse()
        .ok()
        .or_else(|| format!("#{text}").parse().ok())?;
    matches!(value, ArgValue::Color(_)).then_some(value)
}

fn choice_editor(
    binding: &Binding,
    variants: &'static [&'static str],
    segmented: bool,
) -> Result<Editor, LayoutError> {
    let bound = binding.bind(
        0,
        move |value| match value {
            ArgValue::Choice(chosen) => variants
                .iter()
                .position(|variant| variant == chosen)
                .and_then(|index| u32::try_from(index).ok()),
            _ => None,
        },
        move |index| {
            let variant = variants.get(usize::try_from(*index).ok()?)?;
            Some(ArgValue::Choice((*variant).to_string()))
        },
    );
    let name = binding.name;
    let item = match segmented {
        true => tabs(
            TabsProps::props()
                .items(variants.to_vec())
                .selected(bound.shown)
                .build(),
            Children::default(),
        )?,
        false => select(
            SelectProps::props().selected(bound.shown).build(),
            Children::new(move || {
                let mut slots = Slots::new();
                for variant in variants {
                    slots.push(
                        None,
                        item(
                            ItemProps::props().label(*variant).build(),
                            Children::default(),
                        )?,
                    );
                }
                Ok(slots)
            }),
        )?
        .a11y_label(move || name),
    };
    Ok(Editor {
        item,
        current: bound.current,
    })
}

/// The inner control, beside a switch that leaves the arg unset. Turning the switch off sets what the inner control shows.
fn optional_editor(binding: &Binding, inner: ControlKind) -> Result<Editor, LayoutError> {
    let inner = editor(binding, inner)?;
    let set = Rc::clone(&inner.current);
    let unset = binding.bind(
        false,
        |value| Some(matches!(value, ArgValue::Unset)),
        move |unset| match unset {
            true => Some(ArgValue::Unset),
            false => set(),
        },
    );
    let name = binding.name;
    let switch = toggle(
        ToggleProps::props()
            .checked(unset.shown)
            .label(Reactive::of(|| strings::text(UNSET)))
            .build(),
        Children::default(),
    )?
    .a11y_label(move || strings::text_naming(UNSET_ARG, name));
    Ok(Editor {
        item: inline(vec![inner.item, switch])?,
        current: unset.current,
    })
}

fn read_only(binding: &Binding) -> Result<Editor, LayoutError> {
    let state = binding.state;
    let shown = Text::declaring(
        move || state.with(|arg| arg.shown.clone().unwrap_or_else(|| "—".to_string())),
        LayoutStyle::new(),
        mono,
    )?;
    Ok(Editor {
        item: box_item(shown),
        current: Rc::new(|| None),
    })
}

fn inline(items: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID),
        |_| RectStyle::default(),
        items,
    )?;
    Ok(box_item(row))
}

fn field_frame() -> RectStyle {
    let tokens = use_workbench_tokens();
    RectStyle::default()
        .with_fill(tokens.sidebar_background)
        .with_border(Border::uniform(tokens.border_subtle, 1.0))
        .with_radius(BorderRadius::all(WORKBENCH_RADIUS))
}

/// Code: an arg's type, default and value as they are written.
pub(super) fn mono(text: telar::TextStyle) -> telar::TextStyle {
    let tokens = use_workbench_tokens();
    text.with_font_family(tokens.mono_family)
        .with_font_size(tokens.mono_size)
}
