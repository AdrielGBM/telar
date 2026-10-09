//! What a test needs to say about a tree, once instead of once per repository.
//!
//! Every application testing a Telar UI wrote the same three things: mount a tree at a size, find the text in it, and decide whether something actually drew. The last one is the one that goes wrong quietly — an icon-only widget draws a `Path` and no box, so a "did this render?" check that counts boxes reports it blank on a machine where it renders perfectly.
//!
//! The events a test drives a tree with live here too — a press, a release, a key — with [`route`], which hands one to the tree the way the runner does, and [`advance_time`], which lets a timer's wait pass.

use std::time::Duration;

use geometry_core::Rect;
use renderer_core::DrawCommand;
use renderer_core::culling::{FontMetrics, command_visual_rect};
use renderer_core::dirty::FrameDiff;
use ui_core::{Component, ComponentList};

use crate::{
    Event, Key, LayoutStyle, ModifiersState, NamedKey, NodeId, PointerButton, PointerSource,
};

/// Where a surface is repainted going from the frame `old` to the frame `new`, in window space: the regions the renderers' shared diff damages, a scroll's whole clip where the change inside it only scrolled, and nothing where nothing visible changed. `None` where the change reaches the whole surface.
///
/// The same diff the software presenter reports to the compositor, before it is rounded out to whole pixels, so a test can say "this change repaints only that box" of a tree it built without a window.
pub fn damage(new: &[DrawCommand], old: &[DrawCommand]) -> Option<Vec<Rect>> {
    let metrics = FontMetrics::default();
    let change = FrameDiff::default().compare(new, old, |command, matrix| {
        command_visual_rect(command, matrix, &metrics)
    });
    match change.scroll {
        Some(scroll) => Some(
            std::iter::once(scroll.scroll_clip)
                .chain(scroll.extra_dirty)
                .collect(),
        ),
        None => change.damage.map(|rects| rects.into_vec()),
    }
}

/// Mounts `root` and lays it out against a `width`×`height` window on a surface that size, which is what the runner's first `WindowResized` does — a percent-sized tree resolves to nothing until something hands it a definite space.
pub fn mount<C: Component + 'static>(root: C, width: u32, height: u32) -> ComponentList {
    ui_core::set_surface_size(geometry_core::Size::new(width as f32, height as f32));
    let mut tree = ComponentList::new(root);
    crate::batch(|| tree.on_event(&platform_core::Event::WindowResized { width, height }));
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

/// A primary mouse button going down at `(x, y)`.
pub fn press(x: impl Into<f64>, y: impl Into<f64>) -> Event {
    Event::PointerPressed {
        x: x.into(),
        y: y.into(),
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

/// A primary mouse button coming up at `(x, y)`.
pub fn release(x: impl Into<f64>, y: impl Into<f64>) -> Event {
    Event::PointerReleased {
        x: x.into(),
        y: y.into(),
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

/// The mouse moving to `(x, y)`.
pub fn moved(x: impl Into<f64>, y: impl Into<f64>) -> Event {
    Event::PointerMoved {
        x: x.into(),
        y: y.into(),
        source: PointerSource::Mouse,
    }
}

/// `key` going down with `modifiers` held.
pub fn key_with(key: Key, modifiers: ModifiersState) -> Event {
    Event::KeyPressed {
        key,
        modifiers,
        unmodified: None,
    }
}

/// `key` going down with no modifier held.
pub fn named(key: NamedKey) -> Event {
    key_with(Key::Named(key), ModifiersState::default())
}

/// Holds exactly the modifiers named, the way the runner feeds the keyboard registry before a dispatch, and answers them for the key events that follow.
pub fn hold(is_shift: bool, is_alt: bool) -> ModifiersState {
    let modifiers = ModifiersState {
        is_shift,
        is_alt,
        ..Default::default()
    };
    crate::observe_keyboard(&Event::ModifiersChanged { modifiers });
    modifiers
}

/// Dispatches like the runner does: the keyboard and pointer registries hear the event first, then the overlays, and only what they ignore reaches the tree — but for a move, which reaches it [`covered`](ui_core::covered) so a box hovered under an overlay hears the pointer go. A widget with an open panel is not reachable any other way — the panel is in the overlay layer, not under the root.
///
/// The whole dispatch runs inside one batch, as the runner's does, so what the handlers write flushes once they have all let go of their widgets. Without it a list rebuilt from a handler re-renders while its widget is still borrowed, misses that render and loses its subscriptions with it.
pub fn route(tree: &mut ComponentList, event: &Event) {
    crate::observe_keyboard(event);
    crate::observe_pointer(event);
    crate::batch(|| {
        if !crate::dispatch_overlays(event) {
            tree.on_event(event);
        } else if matches!(event, Event::PointerMoved { .. }) {
            ui_core::covered(|| tree.on_event(event));
        }
    });
}

/// Lets `by` pass on the timer clock and runs every [`run_after`](crate::run_after) that came due, as the runner's next frame would, without the test waiting it out.
pub fn advance_time(by: Duration) {
    crate::advance_timer_clock(by);
    crate::fire_timers();
}

/// Lays `node` out as the only child of a `width`×`height` column and answers its absolute rect.
pub fn lay_out(node: NodeId, width: f32, height: f32) -> Rect {
    lay_out_in(
        LayoutStyle::new().flex_column().width(width).height(height),
        node,
        width,
        height,
    )
}

/// [`lay_out`] in a row, for a widget whose rect depends on being laid out along the main axis: which axis the root runs along moves the rect a test asserts on.
pub fn lay_out_row(node: NodeId, width: f32, height: f32) -> Rect {
    lay_out_in(
        LayoutStyle::new().flex_row().width(width).height(height),
        node,
        width,
        height,
    )
}

fn lay_out_in(root_style: LayoutStyle, node: NodeId, width: f32, height: f32) -> Rect {
    let rect = crate::track_layout(node).expect("the node is in the layout tree");
    let root = crate::new_container(root_style, &[node]).expect("the root container is created");
    crate::compute_layout(
        root,
        crate::AvailableSpace::Definite(width),
        crate::AvailableSpace::Definite(height),
    )
    .expect("the node lays out");
    rect.get()
}

/// The centre of `rect`, as the coordinates a pointer event takes.
pub fn centre(rect: Rect) -> (f64, f64) {
    (
        (rect.x + rect.width / 2.0) as f64,
        (rect.y + rect.height / 2.0) as f64,
    )
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

#[cfg(test)]
#[path = "testing_test.rs"]
mod tests;
