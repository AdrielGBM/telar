//! Whether a theme change reaches a tree that is already mounted.
//!
//! The claim the whole theme design rests on: a widget reads a token inside its own `view`, so switching the theme re-runs exactly the segments that read it. Checked on both shapes of backend, because only one of them was ever exercised — a document wraps every box in an element, and an element subscribes to more than a rasterised box does.

use telar::{
    Color, ColorScheme, Component, DrawCommand, EventResult, LayoutItem, LayoutStyle, LocalTree,
    RectStyle, Rectangle, RenderNode, ShapeStyle, SystemPreferences, ThemeTokens, UiTree, box_item,
    reset_layout_runtime, use_theme_tokens,
};

fn dark(dark: bool) -> SystemPreferences {
    SystemPreferences {
        color_scheme: Some(if dark {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        }),
        ..SystemPreferences::default()
    }
}

fn set_system_dark(is_dark: bool) {
    telar::set_system_preferences(dark(is_dark));
}

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

/// And whether the *application* is told, which a host that draws other applications' trees is the whole reason for: it has one runtime per loaded dylib, and the store it writes reaches only its own. `Event::SystemPreferencesChanged` is consumed by the runner and never reaches the tree, so this hook is the only place such a host can hear the change and carry it across the boundary.
#[test]
fn an_application_hosting_other_trees_is_told_the_preferences_changed() {
    use std::cell::Cell;
    use std::rc::Rc;

    use telar::{App, AppRuntime, LocalApp};

    struct Host(Rc<Cell<Option<ColorScheme>>>);
    impl App for Host {
        fn root(&self) -> Box<dyn Component> {
            Box::new(Root(box_item(
                Rectangle::new(LayoutStyle::new().width(1.0).height(1.0), || {
                    RectStyle::default().with_fill(use_theme_tokens().surface_alt())
                })
                .unwrap(),
            )))
        }
        fn on_system_preferences(&self, preferences: &SystemPreferences) {
            self.0.set(preferences.color_scheme);
        }
    }

    reset_layout_runtime();
    set_system_dark(false);
    following_the_system();

    let told = Rc::new(Cell::new(None));
    let app = LocalApp(Host(Rc::clone(&told)));
    app.set_system_preferences(&dark(true));

    let repainted = drawn_fill(&a_themed_box());
    set_system_dark(false);
    assert_eq!(
        told.get(),
        Some(ColorScheme::Dark),
        "the application was never told, so a host could not fan the change out to its plugins"
    );
    assert_eq!(
        repainted,
        Night.surface_alt(),
        "and the default's own work still ran, so the host's tree follows too"
    );
}

#[derive(Clone)]
struct Accent(Color);
impl ThemeTokens for Accent {
    fn primary(&self) -> Color {
        self.0
    }
    fn ink(&self) -> Color {
        self.0
    }
}

/// One subtree of the scene: a box painted from the accent in force and a label inheriting its ink, counting the box's renders.
fn themed_card(
    renders: std::rc::Rc<std::cell::Cell<u32>>,
) -> Result<Box<dyn LayoutItem>, telar::LayoutError> {
    let swatch = Rectangle::new(LayoutStyle::new().width(40.0).height(20.0), move || {
        renders.set(renders.get() + 1);
        RectStyle::default().with_fill(use_theme_tokens().primary())
    })?;
    let label = telar::Text::declaring(|| "card".to_string(), LayoutStyle::new(), |t| t)?;
    Ok(box_item(telar::Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(swatch), box_item(label)],
    )?))
}

fn drawn(tree: &LocalTree) -> (Vec<Color>, Vec<Color>) {
    let frame = tree.frame();
    let fills = frame
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Rect { style, .. } => style.fill.as_ref().map(|p| p.solid_color()),
            _ => None,
        })
        .collect();
    let inks = frame
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { style, .. } => Some(style.color.solid_color()),
            _ => None,
        })
        .collect();
    (fills, inks)
}

/// A bar and a differently themed card in one window, each in its own theme, with no per-surface `set_theme` between them.
#[test]
fn sibling_subtrees_in_one_window_keep_their_own_themes() {
    use std::cell::Cell;
    use std::rc::Rc;

    let bar_accent = Color::rgba(0.1, 0.2, 0.3, 1.0);
    let card_accent = Color::rgba(0.9, 0.5, 0.1, 1.0);
    let switched = Color::rgba(0.2, 0.8, 0.4, 1.0);

    reset_layout_runtime();
    telar::set_theme(Accent(Color::rgba(0.5, 0.5, 0.5, 1.0)));
    let bar_renders = Rc::new(Cell::new(0));
    let card_renders = Rc::new(Cell::new(0));
    let card_theme = telar::ScopedTheme::new(Accent(card_accent));
    let bar = {
        let renders = Rc::clone(&bar_renders);
        telar::provide_theme(Accent(bar_accent), move || themed_card(renders)).unwrap()
    };
    let card = {
        let renders = Rc::clone(&card_renders);
        telar::provide_theme(card_theme, move || themed_card(renders)).unwrap()
    };
    let window = telar::Container::new(
        LayoutStyle::new()
            .flex_row()
            .width(telar::SizeDimension::Percent(1.0))
            .height(telar::SizeDimension::Percent(1.0)),
        vec![Box::new(bar), Box::new(card)],
    )
    .unwrap();
    let mut tree = LocalTree::new(Box::new(telar::WindowRoot::new(box_item(window))));
    tree.on_event(&platform_core::Event::WindowResized {
        width: 200,
        height: 100,
    });

    assert_eq!(
        drawn(&tree),
        (vec![bar_accent, card_accent], vec![bar_accent, card_accent]),
        "each subtree paints and writes in its own theme in the same frame"
    );

    let bar_before = bar_renders.get();
    let card_before = card_renders.get();
    card_theme.set(Accent(switched));
    telar::relayout_if_dirty();
    assert_eq!(
        drawn(&tree),
        (vec![bar_accent, switched], vec![bar_accent, switched])
    );
    assert_eq!(
        (bar_renders.get(), card_renders.get()),
        (bar_before, card_before + 1),
        "switching the card's theme re-renders the card and leaves the bar alone"
    );
}
