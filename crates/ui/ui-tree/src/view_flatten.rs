use renderer_core::DrawCommand;

use crate::render_node::RenderNode;

pub fn flatten_view(
    root: RenderNode,
    out: &mut Vec<DrawCommand>,
    stack: &mut Vec<RenderNode>,
) -> bool {
    stack.clear();
    stack.push(root);
    let mut pos: usize = 0;
    let mut changed = false;

    // emit_cmd checks the slot at `pos` before overwriting to avoid marking changed spuriously
    macro_rules! emit_cmd {
        ($cmd:expr) => {{
            let cmd = $cmd;
            if pos < out.len() {
                if out[pos] != cmd {
                    out[pos] = cmd;
                    changed = true;
                }
            } else {
                out.push(cmd);
                changed = true;
            }
            pos += 1;
        }};
    }

    while let Some(node) = stack.pop() {
        match node {
            RenderNode::Empty => {}
            RenderNode::Primitive(cmd) => emit_cmd!(cmd),
            RenderNode::Group { children } => {
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
            }
            RenderNode::Transform { matrix, children } => {
                stack.push(RenderNode::Primitive(DrawCommand::PopMatrix));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushMatrix { matrix });
            }
            RenderNode::Clip {
                rect,
                radius,
                children,
            } => {
                stack.push(RenderNode::Primitive(DrawCommand::PopClip));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushClip { rect, radius });
            }
            RenderNode::Layer {
                opacity,
                backdrop_blur,
                children,
            } => {
                stack.push(RenderNode::Primitive(DrawCommand::PopLayer));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur
                });
            }
        }
    }

    // truncate stale tail entries left over from a previous longer output
    if pos != out.len() {
        out.truncate(pos);
        changed = true;
    }

    changed
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use renderer_core::{Color, RectStyle, ShapeStyle};
    use std::sync::Arc;

    use super::*;

    fn sample_rect() -> DrawCommand {
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            style: Arc::new(RectStyle::default().with_fill(Color::BLACK)),
        }
    }

    #[test]
    fn flatten_empty_returns_empty() {
        let mut out = Vec::new();
        let mut stack = Vec::new();
        flatten_view(RenderNode::Empty, &mut out, &mut stack);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_group_of_empties() {
        let node = RenderNode::group([RenderNode::Empty, RenderNode::Empty, RenderNode::Empty]);
        let mut out = Vec::new();
        let mut stack = Vec::new();
        flatten_view(node, &mut out, &mut stack);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_nested_groups() {
        let node = RenderNode::group([
            RenderNode::Primitive(sample_rect()),
            RenderNode::group([
                RenderNode::Primitive(sample_rect()),
                RenderNode::Empty,
                RenderNode::group([RenderNode::Primitive(sample_rect())]),
            ]),
            RenderNode::Primitive(sample_rect()),
        ]);
        let mut out = Vec::new();
        let mut stack = Vec::new();
        flatten_view(node, &mut out, &mut stack);
        assert_eq!(out.len(), 4);
    }
}
