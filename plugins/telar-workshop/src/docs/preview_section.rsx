[logic]
use telar::preview::PreviewEntry;
use telar_components::{ButtonProps, ButtonVariant, button};
use telar_devtools::WORKBENCH_GRID;

use super::SECTION_SIZE;
use super::frame::{PreviewFrameProps, preview_frame};
use super::preview_source::{PreviewSourceProps, preview_source};
use crate::state::{ViewMode, WorkshopState};
use crate::strings::{self, OPEN_IN_CANVAS};

/// One preview after the first: its name and a way to open it on the canvas, the preview mounted once it scrolls into view, and its source.
pub struct Props {
    pub state: WorkshopState,
    pub entry: PreviewEntry,
}

let entry = props.entry;
let name = entry.name;
let opening = props.state.clone();

[view]
col gap:WORKBENCH_GRID
    row align:flex-end gap:WORKBENCH_GRID
        col grow:1 min_width:0
            col pad_top:WORKBENCH_GRID role:h3
                text "{name}" font_size:(SECTION_SIZE * 0.875) font_weight:semibold
        button label:(strings::reactive(OPEN_IN_CANVAS)) variant:ButtonVariant::Ghost on_press:(move || { opening.select(entry.id); opening.view().set(ViewMode::Canvas); }) a11y_label:"{strings::text(OPEN_IN_CANVAS)}: {name}"
    preview_frame state:(props.state) entry:entry lazy
    if !entry.source.trim().is_empty()
        preview_source entry:entry
