[logic]
use telar::preview::{PropDefault, PropsSchema};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens};

use crate::strings::{
    self, COLUMN_DEFAULT, COLUMN_DESCRIPTION, COLUMN_NAME, COLUMN_TYPE, REQUIRED,
};

/// The props table: one row per field of the component's props, with its type, what it holds when it is not given and its doc.
pub struct Props {
    pub schema: &'static PropsSchema,
}

const NAME_WIDTH: f32 = 140.0;
const TYPE_WIDTH: f32 = 200.0;
const DEFAULT_WIDTH: f32 = 160.0;

/// What the prop holds when the builder is not given it, or that the builder needs it.
fn default_text(default: PropDefault) -> String {
    match default {
        PropDefault::Required => strings::text(REQUIRED),
        PropDefault::TypeDefault => "Default::default()".to_string(),
        PropDefault::Expr(expr) => expr.to_string(),
        _ => String::new(),
    }
}

[style]
@line
    align: flex-start
    gap: WORKBENCH_GRID * 2.0
    padding_x: WORKBENCH_GRID * 1.5
    padding_y: WORKBENCH_GRID
    stroke: use_workbench_tokens().border_subtle
    stroke_bottom: 1

@mono
    font_family: use_workbench_tokens().mono_family
    font_size: use_workbench_tokens().mono_size

@doc
    grow: 1
    basis: 0
    min_width: 160

[view]
col
    row @line
        text "{strings::text(COLUMN_NAME)}" width:NAME_WIDTH shrink:0 color:$theme.muted
        text "{strings::text(COLUMN_TYPE)}" width:TYPE_WIDTH shrink:0 color:$theme.muted
        text "{strings::text(COLUMN_DEFAULT)}" width:DEFAULT_WIDTH shrink:0 color:$theme.muted
        text @doc "{strings::text(COLUMN_DESCRIPTION)}" color:$theme.muted
    for field in props.schema.fields
        row @line
            text @mono "{field.name}" width:NAME_WIDTH shrink:0 font_weight:semibold
            text @mono "{field.ty}" width:TYPE_WIDTH shrink:0 color:$theme.muted
            text @mono "{default_text(field.default)}" width:DEFAULT_WIDTH shrink:0 color:$theme.muted
            text @doc "{field.doc}"
