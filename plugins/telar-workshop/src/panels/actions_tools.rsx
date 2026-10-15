[logic]
use telar::RwSignal;
use telar::preview::ActionLog;
use telar_components::{ButtonProps, ButtonVariant, TextFieldProps, button, text_field};
use telar_devtools::WORKBENCH_GRID;

use crate::strings::{self, CLEAR_ACTIONS, FILTER_ACTIONS};

/// The filter field and the button that empties the log, for the end of the strip while the actions panel is shown.
pub struct Props {
    pub filter: RwSignal<String>,
    pub log: ActionLog,
}

let log = props.log;

[view]
row align:center gap:WORKBENCH_GRID
    text_field value:(props.filter) placeholder:(strings::reactive(FILTER_ACTIONS)) width:200
    button label:(strings::reactive(CLEAR_ACTIONS)) variant:ButtonVariant::Ghost on_press:(move || log.clear())
