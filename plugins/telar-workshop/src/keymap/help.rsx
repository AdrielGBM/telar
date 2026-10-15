[logic]
use telar::RwSignal;
use telar_components::{KbdProps, ModalProps, kbd, modal};
use telar_devtools::{WORKBENCH_GRID, use_workbench_tokens};

use super::{ACTIONS, Action, GROUPS};
use crate::strings::{self, KEYBOARD_SHORTCUTS};

/// The shortcuts overlay `?` opens: every action that has a shortcut, under its group, with its keys on caps.
pub struct Props {
    pub open: RwSignal<bool>,
}

const WIDTH: f32 = WORKBENCH_GRID * 52.0;
const MAX_HEIGHT: f32 = WORKBENCH_GRID * 60.0;

/// Each group that has an action with a shortcut, with those actions in the order the palette lists them.
fn sections() -> Vec<(&'static str, Vec<Action>)> {
    GROUPS
        .into_iter()
        .map(|group| {
            let actions = ACTIONS
                .into_iter()
                .filter(|action| action.group() == group && !action.chords().is_empty())
                .collect();
            (group, actions)
        })
        .filter(|(_, actions): &(&str, Vec<Action>)| !actions.is_empty())
        .collect()
}

[view]
modal open:props.open title:(strings::reactive(KEYBOARD_SHORTCUTS))
    scroll axis:col max_height:MAX_HEIGHT label:(strings::text(KEYBOARD_SHORTCUTS))
        col width:WIDTH gap:(WORKBENCH_GRID / 2.0)
            for (group, actions) in sections()
                text "{strings::text(&group)}" padding_top:WORKBENCH_GRID font_weight:semibold color:(use_workbench_tokens().text_muted)
                for action in actions
                    row align:center gap:WORKBENCH_GRID
                        text "{strings::text(action.label())}" grow:1 min_width:0
                        for chord in action.chords()
                            kbd chord:(chord.to_string())
