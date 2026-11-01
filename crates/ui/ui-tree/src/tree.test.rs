use geometry_core::Rect;
use reactive_core::signal;
use renderer_core::{Color, RectStyle, ShapeStyle};
use std::sync::Arc;

use super::*;
use crate::render_node::RenderNode;

fn sample_rect(x: f32) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, 0.0, 10.0, 10.0),
        style: Arc::new(RectStyle::default().with_fill(Color::BLACK)),
    }
}

struct Fixed;

impl Component for Fixed {
    fn view(&self) -> RenderNode {
        RenderNode::group([
            RenderNode::Primitive(sample_rect(0.0)),
            RenderNode::Primitive(sample_rect(20.0)),
        ])
    }
}

#[test]
fn tree_initial_render() {
    let tree = ComponentList::new(Fixed);
    let cmds = tree.commands();
    assert_eq!(cmds.len(), 2);
}

struct Counter {
    value: reactive_core::RwSignal<i32>,
}

impl Component for Counter {
    fn view(&self) -> RenderNode {
        let n = self.value.get();
        RenderNode::group((0..n).map(|i| RenderNode::Primitive(sample_rect(i as f32 * 10.0))))
    }
}

#[test]
fn tree_reactive_update() {
    let signal = signal(2i32);
    let tree = ComponentList::new(Counter { value: signal });

    assert_eq!(tree.commands().len(), 2);

    signal.set(5);
    assert_eq!(tree.commands().len(), 5);

    signal.set(0);
    assert_eq!(tree.commands().len(), 0);
}
