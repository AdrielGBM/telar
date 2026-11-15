//! A control at rest has to be told apart from the page it sits on: whatever frames it stands 3:1 from the surface, in either mode.

use telar::{
    AvailableSpace, Color, ColorScheme, ComponentList, DrawCommand, LayoutError, LayoutItem,
    LayoutStyle, NON_TEXT_CONTRAST, Variant, compute_layout, register_mode, relayout_if_dirty,
    set_mode, set_mode_scheme, signal, use_theme_tokens,
};

use crate::test_support::fresh_layout_runtime;

struct Probe {
    name: &'static str,
    build: fn() -> Result<Box<dyn LayoutItem>, LayoutError>,
}

fn probes() -> Vec<Probe> {
    vec![
        Probe {
            name: "text field",
            build: || crate::text_field(crate::TextFieldProps::props().label("Name").build()),
        },
        Probe {
            name: "search field",
            build: || crate::search_field(crate::SearchFieldProps::props().build()),
        },
        Probe {
            name: "unchecked checkbox",
            build: || crate::checkbox(crate::CheckboxProps::props().label("Agree").build()),
        },
        Probe {
            name: "unselected radio",
            build: || {
                crate::radio_group(
                    crate::RadioGroupProps::props()
                        .items(vec!["Large"])
                        .selected(signal(None))
                        .build(),
                )
            },
        },
        Probe {
            name: "off switch",
            build: || crate::toggle(crate::ToggleProps::props().label("Wi-Fi").build()),
        },
        Probe {
            name: "select",
            build: || crate::select(crate::SelectProps::props().build()),
        },
        Probe {
            name: "scrub field",
            build: || crate::scrub_field(crate::ScrubFieldProps::props().label("X").build()),
        },
        Probe {
            name: "unselected filter",
            build: || {
                crate::tag(
                    crate::TagProps::props()
                        .label("Images")
                        .variant(Variant::Solid)
                        .selected(signal(false))
                        .build(),
                )
            },
        },
        Probe {
            name: "swatch the colour of the surface",
            build: || {
                crate::swatches(
                    crate::SwatchesProps::props()
                        .colors(vec![use_theme_tokens().surface()])
                        .names(vec!["Paper".into()])
                        .build(),
                )
            },
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

fn frames(tree: &ComponentList) -> Vec<Color> {
    for _ in 0..3 {
        relayout_if_dirty();
        let _ = tree.commands();
    }
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { style, .. } => {
                style.border.map(|border| border.paint.solid_color())
            }
            _ => None,
        })
        .collect()
}

fn in_mode(scheme: ColorScheme) {
    let id = format!("{scheme:?}");
    register_mode(id.clone(), || {});
    set_mode_scheme(id.clone(), scheme);
    set_mode(id);
}

#[test]
fn every_control_at_rest_is_framed_at_3_to_1_against_the_surface_in_either_mode() {
    let mut shortfalls = Vec::new();
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        for probe in probes() {
            fresh_layout_runtime();
            in_mode(scheme);
            telar::focus::clear();
            let tree = mounted((probe.build)().expect("the control builds"));
            let surface = use_theme_tokens().surface();
            let best = frames(&tree)
                .into_iter()
                .map(|frame| frame.over(surface).contrast_ratio(surface))
                .fold(0.0f32, f32::max);
            if best < NON_TEXT_CONTRAST {
                shortfalls.push(format!(
                    "{scheme:?} {}: its strongest frame is {best:.2}:1",
                    probe.name
                ));
            }
        }
    }
    assert!(shortfalls.is_empty(), "{shortfalls:#?}");
}
