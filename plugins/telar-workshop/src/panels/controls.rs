//! The controls panel: one row per arg of the selected preview, with its editor, type, default and doc.

use std::rc::Rc;

use telar::Variant;
use telar::preview::host::{ArgState, Args};
use telar::preview::{ControlKind, PreviewEntry, PropDefault};
use telar::{
    Accessible, AlignItems, Border, ChildSlot, Children, Container, LayoutError, LayoutItem,
    LayoutScrollArea, LayoutStyle, Reactive, ReactiveList, RectStyle, StyledContainer,
    TemplateTrack, Text, box_item, children, fragment_keyed,
};
use telar_components::{ButtonProps, TagProps, TooltipProps, button, tag, tooltip};
use telar_devtools::{
    WORKBENCH_GRID, use_workbench_tokens, workbench_fill, workbench_mono, workbench_muted,
};

use super::editors::{self, ArgView, Binding};
use crate::empty_state::{EmptyState, Example};
use crate::state::WorkshopState;
use crate::strings::{
    self, COLUMN_DEFAULT, COLUMN_DESCRIPTION, COLUMN_NAME, COLUMN_VALUE, CONTROLS, NO_ARGS,
    NO_ARGS_HINT, NOTHING_SELECTED, RESET, RESET_ARG,
};

const DOC_MIN_WIDTH: f32 = 160.0;
const DOC_LINES: u16 = 2;

const ARG_EXAMPLES: &[Example] = &[
    Example {
        language: "Rust",
        code: ".label(p.arg(\"label\", \"Save\"))",
    },
    Example {
        language: ".rsx",
        code: "[preview \"Default\"]\nbutton label:\"Save\"",
    },
];

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
            None => EmptyState::new(NOTHING_SELECTED).build(),
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
            false => EmptyState::new(NO_ARGS)
                .about(NO_ARGS_HINT)
                .examples(ARG_EXAMPLES)
                .build(),
        },
    )?;
    Ok(box_item(controls))
}

/// How many args the selected preview has, for the panel's tab; `0` for none or no preview.
fn arg_count(state: &WorkshopState) -> usize {
    state
        .selected()
        .map_or(0, |entry| state.args(&entry).states().len())
}

/// [`arg_count`] for the panel's tab; nothing while there are none.
pub(super) fn count_badge(state: &WorkshopState) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = state.clone();
    let count = ReactiveList::with_style(
        LayoutStyle::row(),
        move || match arg_count(&state) {
            0 => Vec::new(),
            count => vec![count],
        },
        |count: &usize| *count,
        |count| {
            tag(TagProps::props()
                .label(Reactive::of(move || count.to_string()))
                .build())
        },
    )?;
    Ok(box_item(count))
}

/// The controls of the preview `args` belong to, at the height of their rows, for a page that scrolls them with the rest of it. Nothing until the preview reads an arg.
pub(crate) fn arg_table(args: Args) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let listed = args.clone();
    let table = ReactiveList::with_style(
        LayoutStyle::column(),
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
    let scroll = LayoutScrollArea::new(LayoutStyle::new().flex_grow(1.0), table)?
        .stop_label(|| strings::text(CONTROLS));
    Ok(box_item(scroll))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Column {
    Name,
    Value,
    Default,
    Doc,
    Reset,
}

impl Column {
    const ALL: [Column; 5] = [
        Column::Name,
        Column::Value,
        Column::Default,
        Column::Doc,
        Column::Reset,
    ];

    fn heading(self) -> Option<&'static str> {
        match self {
            Column::Name => Some(COLUMN_NAME),
            Column::Value => Some(COLUMN_VALUE),
            Column::Default => Some(COLUMN_DEFAULT),
            Column::Doc => Some(COLUMN_DESCRIPTION),
            Column::Reset => None,
        }
    }

    /// The columns grow to their widest cell and never shrink below what their content needs, so a table too wide for the panel scrolls rather than overlapping; an editor is never cut at all, and the doc takes the rest and gives it up first.
    fn track(self) -> TemplateTrack {
        match self {
            Column::Doc => {
                TemplateTrack::minmax(TemplateTrack::px(DOC_MIN_WIDTH), TemplateTrack::fr(1.0))
            }
            Column::Value => TemplateTrack::max_content(),
            _ => TemplateTrack::auto(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ArgCell {
    column: Column,
    pub(super) arg: ArgState,
}

/// One grid, its cells laid straight into it rather than into a box per row, so each column takes the width of its widest cell across every row.
fn table(args: Args) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let listed = args.clone();
    let mut slots = Column::ALL
        .into_iter()
        .map(|column| heading(column).map(ChildSlot::stat))
        .collect::<Result<Vec<_>, _>>()?;
    slots.push(fragment_keyed(
        move || {
            listed
                .states()
                .into_iter()
                .flat_map(|arg| {
                    Column::ALL.map(|column| ArgCell {
                        column,
                        arg: arg.clone(),
                    })
                })
                .collect()
        },
        |cell: &ArgCell| {
            (
                cell.arg.name,
                format!("{:?}", cell.arg.control),
                cell.column,
            )
        },
        move |cell| {
            let column = cell.peek().column;
            arg_cell(
                column,
                Binding {
                    args: args.clone(),
                    name: cell.peek().arg.name,
                    state: ArgView::new(cell),
                },
            )
        },
        0.0,
    ));
    let table = Container::from_slots(
        LayoutStyle::new()
            .display_grid()
            .grid_template_columns(Column::ALL.map(Column::track).to_vec()),
        slots,
    )?;
    Ok(box_item(table))
}

fn heading(column: Column) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = match column.heading() {
        Some(key) => vec![box_item(Text::declaring(
            move || strings::text(key),
            LayoutStyle::new(),
            workbench_muted,
        )?)],
        None => Vec::new(),
    };
    cell(column, label)
}

fn arg_cell(column: Column, binding: Binding) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let state = binding.state;
    let content = match column {
        Column::Name => {
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
            box_item(Container::new(
                LayoutStyle::new().flex_column().gap(WORKBENCH_GRID / 4.0),
                children![name, ty],
            )?)
        }
        Column::Value => editors::editor(&binding, state.peek().control)?.item,
        Column::Default => box_item(Text::declaring(
            move || state.with(default_text),
            LayoutStyle::new(),
            |text| workbench_muted(workbench_mono(text)),
        )?),
        Column::Doc => match state.peek() {
            arg if arg.doc.is_empty() => return cell(column, Vec::new()),
            arg => doc(arg.name, arg.doc)?,
        },
        Column::Reset => reset(&binding)?,
    };
    cell(column, vec![content])
}

/// The arg's doc, cut to as many lines as the name and type take so a long one does not stretch its row, with the whole of it in a tooltip.
fn doc(name: &'static str, doc: &'static str) -> Result<Box<dyn LayoutItem>, LayoutError> {
    tooltip(
        TooltipProps::props()
            .text(Reactive::of(move || name.to_string()))
            .description(Reactive::of(move || doc.to_string()))
            .stretch(true)
            .children(Children::new(move || {
                let text = Text::declaring(
                    move || doc.to_string(),
                    LayoutStyle::new()
                        .flex_grow(1.0)
                        .flex_basis(0.0)
                        .min_width(0.0),
                    |text| workbench_muted(text).with_clamp(DOC_LINES, true),
                )?;
                Ok(vec![box_item(text)])
            }))
            .build(),
    )
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
        LayoutStyle::row(),
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
            .variant(Variant::Ghost)
            .on_press(on_press)
            .build(),
    )?
    .a11y_label(move || strings::text_with(RESET_ARG, name));
    Ok(button)
}

/// A cell of the table, ruled under like the rest of its row and spaced from its neighbours by its own padding, since a gap between the columns would break the rule.
fn cell(
    column: Column,
    content: Vec<Box<dyn LayoutItem>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (gutter, between) = (WORKBENCH_GRID * 1.5, WORKBENCH_GRID);
    let start = match column {
        Column::Name => gutter,
        _ => between,
    };
    let end = match column {
        Column::Reset => gutter,
        _ => between,
    };
    let cell = StyledContainer::new(
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .padding_start(start)
            .padding_end(end)
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
        content,
    )?;
    Ok(box_item(cell))
}
