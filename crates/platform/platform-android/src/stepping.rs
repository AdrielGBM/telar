//! How a screen reader on Android adjusts a value control: it scrolls a slider forward to raise it and backward to lower it, and the adapter hands those on as AccessKit's scroll actions, never as its increment and decrement. So a control that steps offers the scrolls too, and a scroll that comes back is the step it stands for.

use platform_accesskit::accesskit::{Action, ActionRequest, TreeUpdate};

/// Offers the reader's adjustment on every node that steps, so the reader announces it as adjustable and its swipes reach it.
pub(crate) fn offer_steps_as_scrolls(update: &mut TreeUpdate) {
    for (_, node) in &mut update.nodes {
        if node.supports_action(Action::Increment) {
            node.add_action(Action::ScrollUp);
            node.add_action(Action::ScrollDown);
        }
    }
}

/// The step a scroll from the reader stands for: forward, down or right, raises; backward, up or left, lowers. Only a node that steps offers a scroll, so every scroll is one.
pub(crate) fn as_step(request: ActionRequest) -> ActionRequest {
    let action = match request.action {
        Action::ScrollDown | Action::ScrollRight => Action::Increment,
        Action::ScrollUp | Action::ScrollLeft => Action::Decrement,
        _ => return request,
    };
    ActionRequest {
        action,
        data: None,
        ..request
    }
}

#[cfg(test)]
#[path = "stepping.test.rs"]
mod tests;
