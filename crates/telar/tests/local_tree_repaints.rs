//! Whether a tree in this process needs anything forcing it to re-render.
//!
//! `bump_force_ticks` re-runs *every* segment's view effect by setting one signal they all read. It exists
//! for the case where a tree's effects and the signals they read live in different reactive runtimes — a
//! dylib mounted by the host — where no subscription is ever established and nothing wakes on its own.
//!
//! A [`LocalTree`] is by construction not that case, and the trait says so: "Only a tree whose signals live
//! in another runtime than its effects needs this; one that owns both re-renders from its own subscriptions
//! and leaves it a no-op." This is that claim, checked on the two things a frame depends on — the state a
//! widget reads, and the layout it is placed by.

use telar::{
    Color, Component, DrawCommand, EventResult, LayoutItem, LayoutStyle, LocalTree, RectStyle,
    Rectangle, RenderNode, ShapeStyle, SizeDimension, Text, TextStyle, UiTree, WindowRoot,
    box_item, reset_layout_runtime, signal,
};

struct Root(Box<dyn LayoutItem>);
impl Component for Root {
    fn view(&self) -> RenderNode {
        self.0.view()
    }
    fn on_event(&mut self, _: &platform_core::Event) -> EventResult {
        EventResult::Ignored
    }
    fn debug_name(&self) -> &'static str {
        "LocalRoot"
    }
}

fn drawn_text(tree: &LocalTree) -> String {
    tree.frame()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .expect("the tree drew its label")
}

fn drawn_rect(tree: &LocalTree) -> geometry_core::Rect {
    tree.frame()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("the tree drew its box")
}

/// A signal a widget reads is the ordinary case, and the one the segment design exists for.
#[test]
fn a_changed_signal_repaints_without_being_told_to() {
    reset_layout_runtime();
    let label = signal("before".to_string());
    let read = label.clone();
    let text = Text::new(
        move || read.get(),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap();
    let tree = LocalTree::new(Box::new(Root(box_item(text))));

    assert_eq!(drawn_text(&tree), "before");
    label.set("after".to_string());
    telar::relayout_if_dirty();
    assert_eq!(drawn_text(&tree), "after");
}

/// The harder half: a resize changes no signal the widget reads, only the rect it is placed at. A segment
/// that took its rect without subscribing would keep drawing at the old one, which is the failure the force
/// tick was covering for. Driven through a `WindowRoot`, which is what turns a resize into a layout.
#[test]
fn a_resize_repaints_without_being_told_to() {
    reset_layout_runtime();
    let boxed = Rectangle::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(40.0),
        || RectStyle::default().with_fill(Color::BLACK),
    )
    .unwrap();
    let mut tree = LocalTree::new(Box::new(WindowRoot::new(box_item(boxed))));

    tree.on_event(&platform_core::Event::WindowResized {
        width: 400,
        height: 300,
    });
    assert_eq!(drawn_rect(&tree).width, 400.0);

    tree.on_event(&platform_core::Event::WindowResized {
        width: 900,
        height: 300,
    });
    assert_eq!(
        drawn_rect(&tree).width,
        900.0,
        "the box follows the window without a force tick"
    );
}
