[logic]
use std::rc::Rc;

use telar_components::metrics::control_height;
use telar_components::{TooltipProps, tooltip};
use telar_devtools::{WORKBENCH_GRID, WORKBENCH_RADIUS, use_workbench_tokens};
use telar_icons::{IconProps, icon};

use super::FIND_WIDTH;
use crate::arrangement::FIND_FIELD_COMFORT;
use crate::keymap::{Action, Keymap};
use crate::strings::{self, FIND, FIND_IN};

/// Drawn as the field the palette searches in, with the project's name in it, and pressed to open the palette, as the shortcut its tooltip names does from anywhere a field is not taking text.
pub struct Props {
    pub keymap: Keymap,
    pub project: Rc<dyn Fn() -> String>,
}

let keymap = props.keymap;
let project = props.project;
let shown = Rc::clone(&project);

[view]
row align:center width:FIND_WIDTH shrink:1 min_width:FIND_FIELD_COMFORT
    tooltip text:(strings::reactive(FIND)) shortcut:(Action::Find.chords()[0].to_string()) stretch
        row align:center justify:center grow:1 min_width:0 height:(control_height()) pad_x:WORKBENCH_GRID gap:WORKBENCH_GRID fill:(use_workbench_tokens().sidebar_background) stroke:(use_workbench_tokens().border_subtle) stroke_width:1 radius:WORKBENCH_RADIUS hover_style(fill:(use_workbench_tokens().canvas_backdrop)) role:button label:(strings::text_with(FIND_IN, &project())) on_press:(move || keymap.run(Action::Find)) holds_stroke
            icon name:"lucide:search" size:(WORKBENCH_GRID * 2.0) color:$theme.muted
            text "{shown()}" min_width:0 color:$theme.muted text_wrap:nowrap clip:(Clip::x())
