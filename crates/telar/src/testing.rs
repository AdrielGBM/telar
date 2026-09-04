//! What a test needs to say about a tree, once instead of once per repository.
//!
//! Every application testing a Telar UI wrote the same three things: mount a tree at a size, find the text in it, and decide whether something actually drew. The last one is the one that goes wrong quietly — an icon-only widget draws a `Path` and no box, so a "did this render?" check that counts boxes reports it blank on a machine where it renders perfectly.

use geometry_core::Rect;
use renderer_core::DrawCommand;
use ui_core::{Component, ComponentList};

/// Mounts `root` and lays it out against a `width`×`height` window, which is what the runner's first `WindowResized` does — a percent-sized tree resolves to nothing until something hands it a definite space.
pub fn mount<C: Component + 'static>(root: C, width: u32, height: u32) -> ComponentList {
    let mut tree = ComponentList::new(root);
    tree.on_event(&platform_core::Event::WindowResized { width, height });
    tree
}

/// Every string the tree draws, in draw order.
pub fn texts(tree: &ComponentList) -> Vec<String> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

/// Whether the tree draws `needle` anywhere, as a substring of one drawn string.
pub fn find_text(tree: &ComponentList, needle: &str) -> bool {
    texts(tree).iter().any(|text| text.contains(needle))
}

/// The rect of the first command drawing `needle`.
pub fn rect_of(tree: &ComponentList, needle: &str) -> Option<Rect> {
    tree.commands().iter().find_map(|command| match command {
        DrawCommand::Text { rect, text, .. } if text.contains(needle) => Some(*rect),
        _ => None,
    })
}

/// The **layout box** a command is answerable for, and whether it has any content to put there — an empty `Text` shapes to nothing and is the one zero-area draw that is not a fault.
///
/// Deliberately narrower than [`paints`]: only a box the layout produced can be said to have collapsed. An icon's own geometry is the artwork's business — a signal-strength glyph draws its bars as filled slivers a third of a pixel wide, and there is nothing wrong with that.
pub fn painted_rect(command: &DrawCommand) -> Option<Rect> {
    match command {
        DrawCommand::Rect { rect, .. } | DrawCommand::Image { rect, .. } => Some(*rect),
        DrawCommand::Text { rect, text, .. } => (!text.is_empty()).then_some(*rect),
        // A viewport clipped to nothing is the canonical shape of the bug this distinction exists for: the content inside keeps its own honest rects and is cut away wholesale, so only the clip shows the fault.
        DrawCommand::PushClip { rect, .. } => Some(*rect),
        _ => None,
    }
}

/// Whether this command puts ink on the screen at all.
///
/// Wider than [`painted_rect`] by exactly the two commands that carry artwork: an icon-only widget draws a path and nothing else. Counting only boxes makes those widgets invisible to a test rather than measured by it, which is why "did this draw anything" reports them blank on a machine where they render perfectly.
pub fn paints(command: &DrawCommand) -> bool {
    match command {
        DrawCommand::Path { data, .. } => data.bounds().is_some(),
        DrawCommand::Line { .. } => true,
        other => painted_rect(other).is_some(),
    }
}

/// An empty value counts as unset: a workflow that picks the variable per matrix leg still defines it as `""` on the legs that do not want it, and reading that as "required" would fail every skip.
fn gpu_required() -> bool {
    std::env::var("TELAR_REQUIRE_GPU").is_ok_and(|v| !v.is_empty())
}

/// Skips `what` for want of a GPU adapter — unless `TELAR_REQUIRE_GPU` is set, which turns a missing adapter into a failure.
///
/// A pixel-exact test that opens with `let Ok(gpu) = gpu::open() else { return }` passes having asserted nothing on a machine with no adapter, which is every CI runner that was not set up for one. This is the guard that makes that a decision rather than an accident, and it is `pub` because the tests relying on it most are the ones outside this repository.
pub fn require_gpu(what: &str, error: impl std::fmt::Debug) {
    assert!(
        !gpu_required(),
        "{what}: TELAR_REQUIRE_GPU is set, so an adapter was expected: {error:?}"
    );
    eprintln!("skipping {what}: no GPU adapter available: {error:?}");
}
