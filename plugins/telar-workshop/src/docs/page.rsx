[logic]
use telar::preview::PreviewEntry;
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens};
use telar_project::protocol::title_path;

use super::frame::{PreviewFrameProps, preview_frame};
use super::preview_section::{PreviewSectionProps, preview_section};
use super::preview_source::{PreviewSourceProps, preview_source};
use super::props_table::{PropsTableProps, props_table};
use super::{
    PAGE_MARGIN, PAGE_MAX_WIDTH, PreviewControlsProps, SECTION_SIZE, TITLE_SIZE, paragraphs,
    preview_controls,
};
use crate::state::WorkshopState;
use crate::strings::{self, PREVIEWS, PROPS, VIEW_DOCS};

/// The page of one component: its name over the group it is listed under, the prose its previews carry and the props' own doc, its first preview with the controls of its args and its source, the props, and every other preview.
pub struct Props {
    pub state: WorkshopState,
    pub title: &'static str,
    /// The previews listed under the title, in order, of which there is at least one.
    pub entries: Vec<PreviewEntry>,
}

fn heading(title: &'static str) -> (String, &'static str) {
    let mut path: Vec<&str> = title_path(title).collect();
    let name = path.pop().unwrap_or(title);
    (path.join(" / "), name)
}

let state = props.state;
let primary = props.entries[0];
let others = props.entries[1..].to_vec();
let schema = props
    .entries
    .iter()
    .find_map(|entry| entry.props)
    .map(|props| props());
let (group, name) = heading(props.title);
let docs = props
    .entries
    .iter()
    .map(|entry| entry.docs)
    .find(|docs| !docs.is_empty());
let prose: Vec<String> = docs
    .into_iter()
    .chain(schema.map(|schema| schema.doc))
    .flat_map(paragraphs)
    .collect();

[view]
scroll grow:1 min_width:0 min_height:0 axis:col label:(strings::text(VIEW_DOCS))
    col gap:(WORKBENCH_GRID * 2.0) pad:PAGE_MARGIN max_width:PAGE_MAX_WIDTH
        if !group.is_empty()
            text "{group}" color:(use_workbench_tokens().text_muted)
        col role:h1
            text "{name}" font_size:TITLE_SIZE font_weight:semibold
        for paragraph in prose
            text "{paragraph}"
        preview_frame state:state entry:primary
        preview_controls state:state entry:primary
        if !primary.source.trim().is_empty()
            preview_source entry:primary
        if let Some(schema) = schema.filter(|schema| !schema.fields.is_empty())
            col pad_top:WORKBENCH_GRID role:h2
                text "{strings::text(PROPS)}" font_size:SECTION_SIZE font_weight:semibold
            props_table schema:schema
        if !others.is_empty()
            col pad_top:WORKBENCH_GRID role:h2
                text "{strings::text(PREVIEWS)}" font_size:SECTION_SIZE font_weight:semibold
            for entry in others
                preview_section state:state entry:entry
