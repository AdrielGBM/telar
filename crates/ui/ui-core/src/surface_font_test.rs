use layout_core::{AvailableSpace, LayoutStyle};
use renderer_core::{DrawCommand, FontAsset, TextStyle};

use super::*;
use crate::context::{
    compute_layout, new_container, relayout_if_dirty, reset_layout_runtime, track_layout,
};
use crate::{Component, LayoutItem, RenderNode, Surface, Text};

const TEST_FACE: &[u8] = include_bytes!("../../../renderer/renderer-text/test-fonts/TelarTest.ttf");
const FACE: &str = "Telar Surface Default";

fn named() -> FontFamily {
    FontFamily::Named(FACE.into())
}

fn drawn_style(node: &RenderNode) -> Option<TextStyle> {
    match node {
        RenderNode::Primitive(DrawCommand::Text { style, .. }) => Some((**style).clone()),
        RenderNode::Transform { children, .. }
        | RenderNode::Group { children }
        | RenderNode::Element { children, .. } => children.iter().find_map(drawn_style),
        _ => None,
    }
}

/// A `text` as `.rsx` builds one: it names no family, so it shapes in whatever the tree above it says.
struct Laid {
    text: Text,
    node: layout_core::NodeId,
}

impl Laid {
    fn new() -> Self {
        reset_layout_runtime();
        let text = Text::declaring(|| "Wide Words".to_string(), LayoutStyle::new(), |t| t).unwrap();
        let node = text.layout_node();
        let root = new_container(LayoutStyle::new().flex_row(), &[node]).unwrap();
        compute_layout(
            root,
            AvailableSpace::Definite(800.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        Self { text, node }
    }

    fn width(&self) -> f32 {
        relayout_if_dirty();
        track_layout(self.node).unwrap().get().width
    }

    fn family(&self) -> FontFamily {
        drawn_style(&self.text.view())
            .expect("a text leaf draws text")
            .font_family
    }
}

/// Adds the test face to the platform's, rather than making it the only face there is: a database first loaded by `add_faces` holds that face alone.
fn with_face() {
    renderer_text::fonts::installed();
    renderer_text::fonts::add_faces(vec![FontAsset::embedded(TEST_FACE).named(FACE)]);
}

/// The whole point: a text already laid out and drawn takes the surface's new family on the next layout, with nothing rebuilt — and is measured in it, not only drawn in it.
#[test]
fn a_text_laid_out_before_the_family_changes_is_measured_and_drawn_in_the_new_one() {
    with_face();
    let surface = Surface::new();
    let _entered = surface.enter();
    let laid = Laid::new();
    let platform = laid.width();
    assert_eq!(laid.family(), FontFamily::SansSerif);

    set_font_family(Some(named()));
    let face = laid.width();
    assert_eq!(
        laid.family(),
        FontFamily::stack([named(), FontFamily::SansSerif]),
        "drawn in the face, with the platform's own behind it"
    );
    assert_ne!(face, platform, "and measured in it");

    set_font_family(Some(FontFamily::Monospace));
    assert_eq!(laid.family(), FontFamily::Monospace);
    assert_ne!(laid.width(), face);

    set_font_family(None);
    assert_eq!(laid.family(), FontFamily::SansSerif);
    assert_eq!(laid.width(), platform, "back where it started");
}

/// A family declared on the way down is the author's, and wins over the surface's default the way a stylesheet wins over a browser's default font.
#[test]
fn a_declared_family_still_wins_over_the_surface_default() {
    let surface = Surface::new();
    let _entered = surface.enter();
    reset_layout_runtime();
    set_font_family(Some(named()));
    let text = Text::declaring(
        || "Code".to_string(),
        LayoutStyle::new(),
        |t| t.with_font_family(FontFamily::Monospace),
    )
    .unwrap();
    compute_layout(
        text.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(40.0),
    )
    .unwrap();
    assert_eq!(
        drawn_style(&text.view()).unwrap().font_family,
        FontFamily::Monospace
    );
}

#[test]
fn every_surface_has_a_default_family_of_its_own() {
    let a = Surface::new();
    let b = Surface::new();
    {
        let _entered = a.enter();
        open_surface_font_family(Some(named()));
    }
    {
        let _entered = b.enter();
        assert_eq!(use_font_family(), None, "b was told nothing");
        open_surface_font_family(Some(FontFamily::Serif));
    }
    let _entered = a.enter();
    assert_eq!(use_font_family(), Some(named()));
}

/// The runner tells a surface its configured family every time it builds a tree on it; what the app set since is what it keeps.
#[test]
fn opening_seeds_the_family_and_a_later_opening_keeps_what_the_app_set() {
    let surface = Surface::new();
    let _entered = surface.enter();
    open_surface_font_family(Some(named()));
    assert_eq!(use_font_family(), Some(named()));
    open_surface_font_family(Some(FontFamily::Serif));
    assert_eq!(
        use_font_family(),
        Some(FontFamily::Serif),
        "a configuration opened again is followed while the app has said nothing"
    );

    set_font_family(Some(FontFamily::Monospace));
    open_surface_font_family(Some(named()));
    assert_eq!(use_font_family(), Some(FontFamily::Monospace));
}

#[test]
fn only_a_named_face_that_can_be_missing_gets_the_platforms_behind_it() {
    assert_eq!(falling_back(FontFamily::Serif), FontFamily::Serif);
    assert_eq!(
        falling_back(FontFamily::stack([named(), FontFamily::Monospace])),
        FontFamily::stack([named(), FontFamily::Monospace]),
        "a stack that ends in a generic already has a face to land on"
    );
    assert_eq!(
        falling_back(FontFamily::stack([FontFamily::Serif, named()])),
        FontFamily::stack([FontFamily::Serif, named(), FontFamily::SansSerif])
    );
}
