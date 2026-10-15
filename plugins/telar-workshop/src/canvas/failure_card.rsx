[logic]
use telar::preview::PreviewEntry;
use telar_components::{ButtonProps, ButtonVariant, button};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens, workbench_theme};

use super::states::{editor_uri, location};
use crate::state::WorkshopState;
use crate::strings::{self, OPEN_IN_EDITOR, REMOUNT};

/// The card a preview's canvas shows when building it failed: what went wrong, where the preview is written, and a way to open it there or, after a panic, to mount it afresh.
///
/// Built in the workbench theme rather than the application's: the card is the workshop speaking.
pub struct Props {
    pub state: WorkshopState,
    pub entry: PreviewEntry,
    /// The key of the text that names the failure.
    pub heading: &'static str,
    pub detail: String,
    pub panicked: bool,
}

let location = location(&props.entry);
let (file, line) = (props.entry.file, props.entry.line);
let state = props.state;
let detail = props.detail;

[style]
@mono
    font_family: use_workbench_tokens().mono_family
    font_size: use_workbench_tokens().mono_size

[view]
col theme:(workbench_theme()) gap:WORKBENCH_GRID pad:(WORKBENCH_GRID * 2.0) max_width:480 fill:(use_workbench_tokens().panel_background) stroke:(use_workbench_tokens().border_subtle) stroke_width:$theme.border_width radius:$theme.radius_control role:status
    text "{strings::text(props.heading)}" font_weight:semibold color:$theme.error
    text @mono "{detail}"
    if let Some(location) = location.clone()
        text @mono "{location}" color:$theme.muted
    if location.is_some() || props.panicked
        row gap:WORKBENCH_GRID
            if location.is_some()
                button label:(strings::reactive(OPEN_IN_EDITOR)) on_press:(move || { open_uri(&editor_uri(file, line)); })
            if props.panicked
                button label:(strings::reactive(REMOUNT)) variant:ButtonVariant::Ghost on_press:(move || state.remount())
