use std::time::Duration;

use layout_core::LayoutStyle;
use renderer_core::{Color, RectStyle};

use super::*;
use crate::context::reset_layout_runtime;
use crate::layout_item::box_item;
use crate::{Rectangle, StyledContainer};

fn panel() -> Box<dyn LayoutItem> {
    box_item(
        StyledContainer::new(
            LayoutStyle::new().width(100.0).height(40.0),
            |_| RectStyle::filled(Color::BLACK, 0.0),
            vec![],
        )
        .unwrap(),
    )
}

fn opacity(fade: &SurfaceFade) -> f32 {
    match fade.view() {
        RenderNode::Layer { opacity, .. } => opacity,
        _ => 1.0,
    }
}

#[test]
fn a_surface_arrives_see_through_and_settles_opaque() {
    reset_layout_runtime();
    motion_core::reset();
    let fade = SurfaceFade::new(panel(), SurfaceTransition::enter());
    let start = web_time::Instant::now();

    motion_core::tick(start);
    assert!(opacity(&fade) < 0.1, "it opens transparent");

    motion_core::tick(start + Duration::from_millis(400));
    assert_eq!(opacity(&fade), 1.0, "and is drawn as it is once it arrived");
}

#[test]
fn a_surface_sent_back_fades_out_the_way_it_came() {
    reset_layout_runtime();
    motion_core::reset();
    let transition = SurfaceTransition::enter();
    let fade = SurfaceFade::new(panel(), transition.clone());
    let start = web_time::Instant::now();
    motion_core::tick(start);
    motion_core::tick(start + Duration::from_millis(400));

    transition.leave();
    motion_core::tick(start + Duration::from_millis(410));
    motion_core::tick(start + Duration::from_millis(800));

    assert!(opacity(&fade) < 0.1, "it left transparent");
}

#[test]
fn the_fade_adds_no_box_of_its_own() {
    reset_layout_runtime();
    let rectangle = Rectangle::new(LayoutStyle::new().width(10.0).height(10.0), || {
        RectStyle::filled(Color::BLACK, 0.0)
    })
    .unwrap();
    let node = rectangle.layout_node();
    let fade = SurfaceFade::new(box_item(rectangle), SurfaceTransition::enter());
    assert_eq!(fade.layout_node(), node);
}
