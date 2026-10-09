//! The props table: one row per field of the component's props, with its type, what it holds when it is not given and its doc.

use telar::preview::{PropDefault, PropField, PropsSchema};
use telar::{
    AlignItems, Border, LayoutError, LayoutItem, LayoutStyle, RectStyle, StyledContainer, Text,
    TextStyle, box_item,
};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens, workbench_mono, workbench_muted};

use crate::strings::{
    self, COLUMN_DEFAULT, COLUMN_DESCRIPTION, COLUMN_NAME, COLUMN_TYPE, REQUIRED,
};

const NAME_WIDTH: f32 = 140.0;
const TYPE_WIDTH: f32 = 200.0;
const DEFAULT_WIDTH: f32 = 160.0;
const DOC_MIN_WIDTH: f32 = 160.0;

pub(super) fn table(schema: &'static PropsSchema) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut rows = vec![heading()?];
    for field in schema.fields {
        rows.push(row(field)?);
    }
    let table = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_| RectStyle::default(),
        rows,
    )?;
    Ok(box_item(table))
}

fn heading() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = |key: &'static str, style: LayoutStyle| {
        cell(move || strings::text(key), style, workbench_muted)
    };
    line(vec![
        label(COLUMN_NAME, fixed(NAME_WIDTH))?,
        label(COLUMN_TYPE, fixed(TYPE_WIDTH))?,
        label(COLUMN_DEFAULT, fixed(DEFAULT_WIDTH))?,
        label(COLUMN_DESCRIPTION, growing())?,
    ])
}

fn row(field: &'static PropField) -> Result<Box<dyn LayoutItem>, LayoutError> {
    line(vec![
        cell(
            move || field.name.to_string(),
            fixed(NAME_WIDTH),
            |text| workbench_mono(text).with_font_weight(600),
        )?,
        cell(
            move || field.ty.to_string(),
            fixed(TYPE_WIDTH),
            |text| workbench_muted(workbench_mono(text)),
        )?,
        cell(
            move || default_text(field.default),
            fixed(DEFAULT_WIDTH),
            |text| workbench_muted(workbench_mono(text)),
        )?,
        cell(move || field.doc.to_string(), growing(), |text| text)?,
    ])
}

/// What the prop holds when the builder is not given it, or that the builder needs it.
fn default_text(default: PropDefault) -> String {
    match default {
        PropDefault::Required => strings::text(REQUIRED),
        PropDefault::TypeDefault => "Default::default()".to_string(),
        PropDefault::Expr(expr) => expr.to_string(),
        _ => String::new(),
    }
}

fn cell(
    text: impl Fn() -> String + 'static,
    style: LayoutStyle,
    look: fn(TextStyle) -> TextStyle,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Text::declaring(text, style, look)?))
}

fn line(cells: Vec<Box<dyn LayoutItem>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let line = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::FLEX_START)
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
