use std::cell::{Cell, RefCell};
use std::rc::Rc;

use geometry_core::Color;
use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Key, ModifiersState};
use renderer_core::{Declared, DrawCommand, RectStyle, ShapeStyle};
use theme_core::{ThemeTokens, set_theme, use_theme_tokens};
use ui_tree::ComponentList;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::inherit::{declare, inherited_text_style};
use crate::rect::Rectangle;

const RED: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);
const GREEN: Color = Color::rgba(0.0, 1.0, 0.0, 1.0);
const BLUE: Color = Color::rgba(0.0, 0.0, 1.0, 1.0);

#[derive(Clone)]
struct Accent(Color);
impl ThemeTokens for Accent {
    fn primary(&self) -> Color {
        self.0
    }
    fn ink(&self) -> Color {
        self.0
    }
    fn root(&self) -> Declared {
        Declared::default().with_font_size(11.0)
    }
}

/// A box filled with the accent in force, counting its renders.
fn swatch(renders: Rc<Cell<u32>>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Rectangle::new(
        LayoutStyle::new().width(10.0).height(10.0),
        move || {
            renders.set(renders.get() + 1);
            RectStyle::default().with_fill(use_theme_tokens().primary())
        },
    )?))
}

fn fills(tree: &ComponentList) -> Vec<Color> {
    tree.commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Rect { style, .. } => style.fill.as_ref().map(|p| p.solid_color()),
            _ => None,
        })
        .collect()
}

fn lay_out(root: NodeId) {
    compute_layout(
        root,
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();
}

struct Scene {
    tree: ComponentList,
    red: ScopedTheme,
    renders: [Rc<Cell<u32>>; 3],
}

/// Two sibling subtrees in their own themes, and a third in the global one, in one tree.
fn scene() -> Scene {
    reset_layout_runtime();
    set_theme(Accent(BLUE));
    let renders: [Rc<Cell<u32>>; 3] = Default::default();
    let red = ScopedTheme::new(Accent(RED));
    let first = Rc::clone(&renders[0]);
    let second = Rc::clone(&renders[1]);
    let row = Container::new(
        LayoutStyle::new().flex_row().width(100.0).height(20.0),
        vec![
            Box::new(provide_theme(red, move || swatch(first)).unwrap()),
            Box::new(provide_theme(Accent(GREEN), move || swatch(second)).unwrap()),
            swatch(Rc::clone(&renders[2])).unwrap(),
        ],
    )
    .unwrap();
    lay_out(row.layout_node());
    Scene {
        tree: ComponentList::new(row),
        red,
        renders,
    }
}

#[test]
fn sibling_subtrees_draw_in_their_own_themes_in_one_frame() {
    let scene = scene();
    assert_eq!(fills(&scene.tree), vec![RED, GREEN, BLUE]);
}

#[test]
fn switching_one_subtree_re_renders_only_that_subtree() {
    let scene = scene();
    let _ = scene.tree.commands();
    let before: Vec<u32> = scene.renders.iter().map(|r| r.get()).collect();

    scene.red.set(Accent(GREEN));
    let after_scoped: Vec<u32> = scene.renders.iter().map(|r| r.get()).collect();
    assert_eq!(fills(&scene.tree), vec![GREEN, GREEN, BLUE]);
    assert_eq!(
        after_scoped,
        vec![before[0] + 1, before[1], before[2]],
        "only the switched subtree re-renders"
    );

    set_theme(Accent(RED));
    let after_global: Vec<u32> = scene.renders.iter().map(|r| r.get()).collect();
    assert_eq!(fills(&scene.tree), vec![GREEN, GREEN, RED]);
    assert_eq!(
        after_global,
        vec![after_scoped[0], after_scoped[1], after_scoped[2] + 1],
        "the global theme reaches only what no provider covers"
    );
    set_theme(Accent(BLUE));
}

/// The text cascade is walked by layout, outside every owner, and must still start from the provider's root row rather than from what the tree around it declared.
#[test]
fn text_under_a_provider_starts_from_that_theme_not_the_region_around_it() {
    reset_layout_runtime();
    set_theme(Accent(BLUE));
    let themed = ScopedTheme::new(Accent(RED));
    let inner_leaf = Rc::new(Cell::new(None));
    let leaf_cell = Rc::clone(&inner_leaf);
    let provided = provide_theme(themed, move || {
        let leaf = Container::new(LayoutStyle::new(), vec![])?;
        leaf_cell.set(Some(leaf.layout_node()));
        Ok(Box::new(leaf) as Box<dyn LayoutItem>)
    })
    .unwrap();
    let outside = Container::new(LayoutStyle::new(), vec![]).unwrap();
    let outside_node = outside.layout_node();
    let region = Container::new(
        LayoutStyle::new(),
        vec![Box::new(provided), Box::new(outside)],
    )
    .unwrap();
    declare(
        region.layout_node(),
        Declared::default().with_font_size(30.0).with_color(GREEN),
    );
    let inner_leaf = inner_leaf.get().unwrap();

    let inside = inherited_text_style(inner_leaf);
    assert_eq!((inside.font_size, inside.color.solid_color()), (11.0, RED));
    let beside = inherited_text_style(outside_node);
    assert_eq!(
        (beside.font_size, beside.color.solid_color()),
        (30.0, GREEN)
    );

    themed.set(Accent(BLUE));
    assert_eq!(
        inherited_text_style(inner_leaf).color.solid_color(),
        BLUE,
        "the root row follows the provider's theme"
    );
    drop(region);
}

/// A leaf that reads the theme in force when an event reaches it, long after it was built.
struct Listener {
    inner: Container,
    heard: Rc<RefCell<Vec<Color>>>,
}

impl LayoutItem for Listener {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }
}

impl Component for Listener {
    fn view(&self) -> RenderNode {
        self.inner.view()
    }

    fn on_event(&mut self, _event: &Event) -> EventResult {
        self.heard.borrow_mut().push(use_theme_tokens().primary());
        EventResult::Ignored
    }
}

#[test]
fn a_handler_inside_a_provider_reads_the_provider() {
    reset_layout_runtime();
    set_theme(Accent(BLUE));
    let heard = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&heard);
    let provided = provide_theme(Accent(RED), move || {
        Ok(Box::new(Listener {
            inner: Container::new(LayoutStyle::new(), vec![])?,
            heard: sink,
        }) as Box<dyn LayoutItem>)
    })
    .unwrap();
    let mut tree =
        ComponentList::new(Container::new(LayoutStyle::new(), vec![Box::new(provided)]).unwrap());

    tree.on_event(&Event::KeyPressed {
        key: Key::Char('a'),
        modifiers: ModifiersState::default(),
    });
    assert_eq!(*heard.borrow(), vec![RED]);
}

#[test]
fn a_failed_build_frees_the_scope_it_opened() {
    reset_layout_runtime();
    let before = reactive_core::live_signal_count();
    let result = provide_theme(Accent(RED), || Err(LayoutError::Engine("refused".into())));
    assert!(result.is_err());
    assert_eq!(reactive_core::live_signal_count(), before);
}

/// A child that fails partway, by an error or a panic, leaves neither its scope nor the nodes it had built.
#[test]
fn a_failed_build_frees_what_it_built() {
    use crate::test_support::{Failure, live, stateful_leaf};

    let attempt = |fail| {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            provide_theme(Accent(RED), || {
                let _loose = stateful_leaf(Failure::None)?;
                stateful_leaf(fail)
            })
        }));
        assert!(!matches!(outcome, Ok(Ok(_))));
    };
    reset_layout_runtime();
    // The first build on a thread brings up the ambient worlds it touches, which live on.
    attempt(Failure::Err);
    let baseline = live();
    for fail in [Failure::Err, Failure::Panic, Failure::Err] {
        attempt(fail);
        assert_eq!(live(), baseline);
    }
}
