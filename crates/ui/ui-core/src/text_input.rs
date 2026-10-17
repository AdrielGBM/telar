//! Where the field that holds the keyboard has its caret, for the platform to hand to the input method composing into it.

use layout_core::NodeId;
use platform_core::TextInput;

/// The caret of the field that holds the keyboard, where the surface shows it, or `None` while no field does. Read without subscribing, once a frame has drawn the field: a runner hands it to [`Window::set_text_input`](platform_core::Window::set_text_input) whenever it changes.
pub fn text_input() -> Option<TextInput> {
    crate::focus::focused_text_input().and_then(|source| source())
}

/// `local`, a caret in `node`'s own coordinates, placed where the surface shows it.
pub(crate) fn placed(node: NodeId, local: TextInput) -> Option<TextInput> {
    let caret = crate::input_region::surface_rect(node, local.caret)?;
    Some(TextInput { caret, ..local })
}

#[cfg(test)]
#[path = "text_input_test.rs"]
mod tests;
