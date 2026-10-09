//! The controls panel: one row per arg of the selected preview, with its editor, type, default and doc.

use std::rc::Rc;

use telar::preview::host::{ArgState, Args};
use telar::preview::{ControlKind, PreviewEntry, PropDefault};
use telar::{
    Accessible, AlignItems, Border, Children, JustifyContent, LayoutError, LayoutItem,
    LayoutScrollArea, LayoutStyle, Reactive, ReactiveList, RectStyle, StyledContainer, Text,
    box_item,
};
use telar_components::{ButtonProps, button};
use telar_devtools::{
    WORKBENCH_GRID, use_workbench_tokens, workbench_fill, workbench_mono, workbench_muted,
};

use super::editors::{self, Binding};
use crate::state::WorkshopState;
use crate::strings::{
    self, COLUMN_DEFAULT, COLUMN_DESCRIPTION, COLUMN_NAME, COLUMN_VALUE, NO_ARGS, NOTHING_SELECTED,
    RESET, RESET_ARG,
};

const NAME_WIDTH: f32 = 160.0;
const VALUE_WIDTH: f32 = 300.0;
const DEFAULT_WIDTH: f32 = 140.0;
const DOC_MIN_WIDTH: f32 = 160.0;
const RESET_WIDTH: f32 = 72.0;

/// The selected preview's controls, or why there are none.
pub(super) fn controls(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let selected = state.clone();
    let state = state.clone();
    let panel = ReactiveList::with_style(
        workbench_fill(),
        move || vec![selected.selected()],
        |entry: &Option<PreviewEntry>| entry.map(|entry| entry.id),
        move |entry| match entry {
            Some(entry) => preview_controls(state.args(&entry)),
            None => empty(NOTHING_SELECTED),
        },
    )?;
    Ok(box_item(panel))
}

fn preview_controls(args: Args) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let listed = args.clone();
    let controls = ReactiveList::with_style(
        workbench_fill(),
        move || vec![!listed.states().is_empty()],
        |has_args: &bool| *has_args,
        move |has_args| match has_args {
            true => scrolling(table(args.clone())?),
            false => empty(NO_ARGS),
        },
    )?;
    Ok(box_item(controls))
}

/// The controls of the preview `args` belong to, at the height of their rows, for a page that scrolls them with the rest of it. Nothing until the preview reads an arg.
pub(crate) fn arg_table(args: Args) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let listed = args.clone();
    let table = ReactiveList::with_style(
        LayoutStyle::new().flex_column(),
        move || match listed.states().is_empty() {
            true => Vec::new(),
            false => vec![()],
        },
        |_: &()| (),
        move |()| table(args.clone()),
    )?;
    Ok(box_item(table))
}

fn scrolling(table: Box<dyn LayoutItem>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let scroll = LayoutScrollArea::new(LayoutStyle::new().flex_grow(1.0).min_height(0.0), table)?;
    Ok(box_item(scroll))
}

fn table(args: Args) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let listed = args.clone();
    let rows = ReactiveList::keyed(
        move || listed.states(),
        |arg: &ArgState| (arg.name, format!("{:?}", arg.control)),
        move |state| {
            row(Binding {
                args: args.clone(),
                name: state.peek().name,
                state,
            })
        },
    )?;
    let table = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_| RectStyle::default(),
        vec![heading()?, box_item(rows)],
    )?;
    Ok(box_item(table))
}

fn heading() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label =
        |key: &'static str, style: LayoutStyle| -> Result<Box<dyn LayoutItem>, LayoutError> {
            let text = Text::declaring(move || strings::text(key), style, workbench_muted)?;
            Ok(box_item(text))
        };
    line(vec![
        label(COLUMN_NAME, fixed(NAME_WIDTH))?,
        label(COLUMN_VALUE, fixed(VALUE_WIDTH))?,
        label(COLUMN_DEFAULT, fixed(DEFAULT_WIDTH))?,
        label(COLUMN_DESCRIPTION, growing())?,
        spacer(RESET_WIDTH)?,
    ])
}

fn row(binding: Binding) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = binding.state;
    let name = Text::declaring(
        move || state.with(|arg| arg.name.to_string()),
        LayoutStyle::new(),
        |text| text.with_font_weight(600),
    )?;
    let ty = Text::declaring(
        move || state.with(type_text),
        LayoutStyle::new(),
        |text| workbench_muted(workbench_mono(text)),
    )?;
    let name = StyledContainer::new(
        fixed(NAME_WIDTH).flex_column().gap(WORKBENCH_GRID / 4.0),
        |_| RectStyle::default(),
        vec![box_item(name), box_item(ty)],
    )?;
    let editor = editors::editor(&binding, state.peek().control)?;
    let value = StyledContainer::new(
        fixed(VALUE_WIDTH)
            .flex_row()
            .align_items(AlignItems::CENTER),
        |_| RectStyle::default(),
        vec![editor.item],
    )?;
    let default = Text::declaring(
        move || state.with(default_text),
        fixed(DEFAULT_WIDTH),
        |text| workbench_muted(workbench_mono(text)),
    )?;
    let doc = Text::declaring(
        move || state.with(|arg| arg.doc.to_string()),
        growing(),
        workbench_muted,
    )?;
    line(vec![
        box_item(name),
        box_item(value),
        box_item(default),
        box_item(doc),
        reset(&binding)?,
    ])
}

/// The prop's type as the struct declares it, or else what the control says of the value.
fn type_text(arg: &ArgState) -> String {
    match arg.prop {
        Some(prop) => prop.ty.to_string(),
        None => kind_text(&arg.control),
    }
}

fn kind_text(control: &ControlKind) -> String {
    match control {
        ControlKind::Toggle => "bool".to_string(),
        ControlKind::Number(number) if number.integer => "integer".to_string(),
        ControlKind::Number(_) => "float".to_string(),
        ControlKind::Text { .. } => "String".to_string(),
        ControlKind::Color => "Color".to_string(),
        ControlKind::Choice { variants } => variants.join(" | "),
        ControlKind::Optional(inner) => format!("Option<{}>", kind_text(inner)),
        _ => String::new(),
    }
}

/// The default the arg resets to, or else what the prop falls back to when the builder is not given it.
fn default_text(arg: &ArgState) -> String {
    match (&arg.default, arg.prop.map(|prop| prop.default)) {
        (Some(value), _) => value.to_string(),
        (None, Some(PropDefault::Expr(expr))) => expr.to_string(),
        (None, Some(PropDefault::TypeDefault)) => "Default::default()".to_string(),
        _ => "—".to_string(),
    }
}

/// Puts the one arg back to its default, offered only while it holds something else.
fn reset(binding: &Binding) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = binding.state;
    let binding = binding.clone();
    let cell = ReactiveList::with_style(
        fixed(RESET_WIDTH).flex_row(),
        move || match state.with(|arg| arg.edited && arg.default.is_some()) {
            true => vec![()],
            false => Vec::new(),
        },
        |_: &()| (),
        move |()| reset_button(&binding),
    )?;
    Ok(box_item(cell))
}

fn reset_button(binding: &Binding) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let name = binding.name;
    let binding = binding.clone();
    let on_press: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(default) = binding.state.peek().default {
            // A default the arg refuses leaves it as it was, which is all a reset could do anyway.
            let _ = binding.args.set(binding.name, default);
        }
    });
    let button = button(
        ButtonProps::props()
            .label(Reactive::of(|| strings::text(RESET)))
            .ghost(true)
            .on_press(on_press)
            .build(),
        Children::default(),
    )?
    .a11y_label(move || strings::text_with(RESET_ARG, name));
    Ok(button)
}

fn line(cells: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let line = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(WORKBENCH_GRID * 2.0)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .padding_vertical(WORKBENCH_GRID),
        |_| {
            RectStyle::default().with_border(Border::per_side(
                use_workbench_tokens().border_subtle,
                0.0,
                0.0,
                1.0,
                0.0,
            ))
        },
        cells,
    )?;
    Ok(box_item(line))
}

fn fixed(width: f32) -> LayoutStyle {
    LayoutStyle::new().width(width).flex_shrink(0.0)
}

fn growing() -> LayoutStyle {
    LayoutStyle::new()
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_width(DOC_MIN_WIDTH)
}

fn spacer(width: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let spacer = StyledContainer::new(fixed(width), |_| RectStyle::default(), Vec::new())?;
    Ok(box_item(spacer))
}

fn empty(key: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let message = Text::declaring(
        move || strings::text(key),
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let centred = StyledContainer::new(
        workbench_fill()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        |_| RectStyle::default(),
        vec![box_item(message)],
    )?;
    Ok(box_item(centred))
}
