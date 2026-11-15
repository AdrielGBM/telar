//! The keyboard's ring has to show on every control it lands on: on a control filled with the accent the ring is drawn in, and on the surface around a control with no fill of its own.

use telar::{
    AvailableSpace, Color, ColorScheme, ComponentList, DrawCommand, LayoutError, LayoutItem,
    LayoutStyle, NON_TEXT_CONTRAST, RectStyle, Variant, compute_layout, register_mode,
    relayout_if_dirty, set_mode, set_mode_scheme, use_theme_tokens,
};

use crate::test_support::fresh_layout_runtime;

struct Probe {
    name: &'static str,
    build: fn() -> Result<Box<dyn LayoutItem>, LayoutError>,
}

fn probes() -> [Probe; 3] {
    [
        Probe {
            name: "solid button",
            build: || {
                crate::button(
                    crate::ButtonProps::props()
                        .label("Save")
                        .variant(Variant::Solid)
                        .build(),
                )
            },
        },
        Probe {
            name: "outline button",
            build: || {
                crate::button(
                    crate::ButtonProps::props()
                        .label("Save")
                        .variant(Variant::Outline)
                        .build(),
                )
            },
        },
        Probe {
            name: "selected tab",
            build: || crate::tabs(crate::TabsProps::props().items(vec!["One", "Two"]).build()),
        },
    ]
}

fn mounted(item: Box<dyn LayoutItem>) -> ComponentList {
    let window = telar::Container::new(
        LayoutStyle::new()
            .flex_column()
            .align_items(telar::AlignItems::START),
        vec![item],
    )
    .expect("the window builds");
    compute_layout(
        window.layout_node(),
        AvailableSpace::Definite(800.0),
        AvailableSpace::Definite(600.0),
    )
    .expect("the window lays out");
    ComponentList::new(telar::box_item(window))
}

fn rects(tree: &ComponentList) -> Vec<RectStyle> {
    for _ in 0..3 {
        relayout_if_dirty();
        let _ = tree.commands();
    }
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { style, .. } => Some(**style),
            _ => None,
        })
        .collect()
}

fn in_mode(scheme: ColorScheme) {
    let id = format!("{scheme:?}");
    let id = register_mode(id, || {});
    set_mode_scheme(&id, scheme);
    set_mode(&id);
}

/// What the keyboard reaching the first control changes on screen: the paint of the control that wears the ring, and every band of the ring.
fn ring_on_first_control(probe: &Probe) -> (Option<Color>, Vec<Color>) {
    fresh_layout_runtime();
    telar::focus::clear();
    let tree = mounted((probe.build)().expect("the control builds"));
    let before = rects(&tree);
    telar::focus::focus_next();
    let changed: Vec<RectStyle> = rects(&tree)
        .into_iter()
        .filter(|style| !before.contains(style))
        .collect();
    let owner = changed
        .first()
        .unwrap_or_else(|| panic!("{}: focus changes what is drawn", probe.name));
    let bands = changed
        .iter()
        .filter_map(|style| style.border.map(|border| border.paint.solid_color()))
        .collect();
    (owner.fill.map(|fill| fill.solid_color()), bands)
}

#[test]
fn one_band_of_the_ring_stands_out_from_the_control_s_fill_and_from_the_surface_in_either_mode() {
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        in_mode(scheme);
        for probe in probes() {
            let (fill, bands) = ring_on_first_control(&probe);
            assert!(
                bands.len() >= 2,
                "{scheme:?} {}: the ring is drawn as two bands, got {bands:?}",
                probe.name
            );
            let surface = use_theme_tokens().surface();
            let fill = fill.map_or(surface, |fill| fill.over(surface));
            for (what, against) in [("its fill", fill), ("the surface", surface)] {
                let best = bands
                    .iter()
                    .map(|band| band.contrast_ratio(against))
                    .fold(0.0f32, f32::max);
                assert!(
                    best >= NON_TEXT_CONTRAST,
                    "{scheme:?} {}: the ring's better band reaches {best:.2}:1 against {what}",
                    probe.name
                );
            }
        }
    }
    telar::focus::clear();
}
