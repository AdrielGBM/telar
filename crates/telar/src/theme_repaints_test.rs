//! Whether a theme change reaches a tree that is already mounted.
//!
//! The claim the whole theme design rests on: a widget reads a token inside its own `view`, so switching the theme re-runs exactly the segments that read it. Checked on both shapes of backend, because only one of them was ever exercised — a document wraps every box in an element, and an element subscribes to more than a rasterised box does.

use telar::{
    Color, Component, DrawCommand, EventResult, LayoutItem, LayoutStyle, LocalTree, RectStyle,
    Rectangle, RenderNode, ShapeStyle, ThemeTokens, UiTree, box_item, reset_layout_runtime,
    set_system_dark, use_theme_tokens,
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
        "ThemeRoot"
    }
}

#[derive(Clone)]
struct Day;
impl ThemeTokens for Day {
    fn surface_alt(&self) -> Color {
        Color::rgba(1.0, 0.0, 0.0, 1.0)
    }
}

#[derive(Clone)]
struct Night;
impl ThemeTokens for Night {
    fn surface_alt(&self) -> Color {
        Color::rgba(0.0, 0.0, 1.0, 1.0)
    }
}

fn drawn_fill(tree: &LocalTree) -> Color {
    tree.frame()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { style, .. } => style.fill.as_ref().map(|p| p.solid_color()),
            _ => None,
        })
        .expect("the tree drew its box")
}

/// A box painted from a token, mounted, and then handed a different theme.
fn a_themed_box() -> LocalTree {
    let boxed = Rectangle::new(LayoutStyle::new().width(100.0).height(40.0), || {
        RectStyle::default().with_fill(use_theme_tokens().surface_alt())
    })
    .unwrap();
    LocalTree::new(Box::new(Root(box_item(boxed))))
}

fn following_the_system() {
    telar::register_mode("day", || telar::set_theme(Day));
    telar::register_mode("night", || telar::set_theme(Night));
    telar::follow_system("day", "night");
}

#[test]
fn a_theme_change_repaints_a_rasterised_tree() {
    reset_layout_runtime();
    set_system_dark(false);
    following_the_system();
    let tree = a_themed_box();
    assert_eq!(
        drawn_fill(&tree),
        Day.surface_alt(),
        "it opens in the light one"
    );

    set_system_dark(true);
    telar::relayout_if_dirty();
    assert_eq!(
        drawn_fill(&tree),
        Night.surface_alt(),
        "and follows the system"
    );
    set_system_dark(false);
}

/// The same claim where every box is also an element, which is what a document backend asks for.
#[test]
fn a_theme_change_repaints_a_tree_of_elements() {
    reset_layout_runtime();
    let was = ui_tree::set_element_capture(true);
    set_system_dark(false);
    following_the_system();
    let tree = a_themed_box();
    assert_eq!(
        drawn_fill(&tree),
        Day.surface_alt(),
        "it opens in the light one"
    );

    set_system_dark(true);
    telar::relayout_if_dirty();
    let after = drawn_fill(&tree);
    ui_tree::set_element_capture(was);
    set_system_dark(false);
    assert_eq!(after, Night.surface_alt(), "and follows the system");
}

/// The runner's own shape, which is where the ordering actually bites: a backend that can read the OS preference reports it *before* the tree mounts, so the first layout is already in the right theme — and it reports it inside the batch `new_events` opened, so nothing flushes until `about_to_wait` closes it. The tree is therefore built while the theme is still the default one, and only the flush afterwards switches it. Every box has to follow that.
#[test]
fn a_tree_mounted_before_the_flush_still_takes_the_theme_the_flush_installs() {
    reset_layout_runtime();
    let was = ui_tree::set_element_capture(true);
    set_system_dark(false);
    following_the_system();

    telar::begin_batch();
    set_system_dark(true);
    let tree = a_themed_box();
    let during = drawn_fill(&tree);
    telar::end_batch();
    telar::relayout_if_dirty();
    let after = drawn_fill(&tree);

    ui_tree::set_element_capture(was);
    set_system_dark(false);
    assert_eq!(
        during,
        Day.surface_alt(),
        "nothing flushed yet, so the tree is built in the theme that was in force"
    );
    assert_eq!(
        after,
        Night.surface_alt(),
        "and the flush that installs the real one repaints it"
    );
}
