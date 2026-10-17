//! Reading a surface the way the inspection panels and the accessibility checks read it.
//!
//! [`a11y::check`] reports what would keep someone from using one surface, from a [`Frame`] of it; [`audit`] is what a reader is told about a whole surface and what checking it finds, the surfaces nested in its frame included, each read in its own world. The devtools and the workshop show the same audit, of the window and of a canvas, so the two agree wherever they show the same thing.

pub mod a11y;
pub(crate) mod frame;

pub use frame::{Frame, FrameAccess};

use platform_core::AccessAudit;
use ui_core::accessibility::{SurfaceTree, graft, surface_tree};

use crate::{Color, DrawCommand, Size};

/// What a reader is told about the surface entered now, which drew `commands`, and what [`a11y::check`] finds in it — and the same for every surface nested in its frame, each read and checked in its own world and placed where the frame shows it, inside the node it is drawn in.
///
/// `background` is what shows behind everything the commands draw, where it is known. Read with the surface entered and before anything in it changes, as the checks are.
pub fn audit(commands: &[DrawCommand], background: Option<Color>) -> AccessAudit {
    let SurfaceTree { nodes, nested } = surface_tree(commands);
    let frame = Frame::new(commands, Size::ZERO)
        .with_direction(crate::current_direction())
        .with_lang(i18n_core::current_locale())
        .with_background(background)
        .with_access(Some(FrameAccess::with_snapshot(commands, nodes.clone())));
    let mut violations = a11y::check(&frame, &nodes).violations;
    let mut grafts = Vec::new();
    for (nested, parent) in nested {
        let Some(_entered) = nested.enter() else {
            continue;
        };
        let inner = audit(&commands[nested.commands.clone()], None);
        grafts.push((
            parent,
            inner
                .nodes
                .into_iter()
                .map(|node| nested.place(node))
                .collect(),
        ));
        violations.extend(inner.violations.into_iter().map(|violation| {
            let rect = violation.rect.map(|rect| nested.place_rect(rect));
            violation.at(rect)
        }));
    }
    AccessAudit {
        nodes: graft(nodes, grafts),
        violations,
    }
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
