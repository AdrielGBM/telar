use renderer_core::DrawCommand;

use crate::view::View;

pub fn flatten_view(root: View, out: &mut Vec<DrawCommand>, stack: &mut Vec<View>) -> bool {
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

    while let Some(view) = stack.pop() {
        match view {
            View::Empty => {}
            View::Primitive(cmd) => emit_cmd!(cmd),
            View::Group(children) => {
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
            }
            View::Translate { tx, ty, children } => {
                stack.push(View::Primitive(DrawCommand::PopTransform));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushTransform { tx, ty });
            }
            View::Clip { rect, children } => {
                stack.push(View::Primitive(DrawCommand::PopClip));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushClip { rect });
            }
            View::Layer { opacity, children } => {
                stack.push(View::Primitive(DrawCommand::PopLayer));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushLayer { opacity });
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
    use renderer_core::{Color, RectStyle};

    use super::*;

    fn sample_rect() -> DrawCommand {
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            style: RectStyle::default().with_fill(Color::BLACK),
        }
    }

    #[test]
    fn flatten_empty_returns_empty() {
        let mut out = Vec::new();
        let mut stack = Vec::new();
        flatten_view(View::Empty, &mut out, &mut stack);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_group_of_empties() {
        let view = View::group([View::Empty, View::Empty, View::Empty]);
        let mut out = Vec::new();
        let mut stack = Vec::new();
        flatten_view(view, &mut out, &mut stack);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_nested_groups() {
        let view = View::group([
            View::Primitive(sample_rect()),
            View::group([
                View::Primitive(sample_rect()),
                View::Empty,
                View::group([View::Primitive(sample_rect())]),
            ]),
            View::Primitive(sample_rect()),
        ]);
        let mut out = Vec::new();
        let mut stack = Vec::new();
        flatten_view(view, &mut out, &mut stack);
        assert_eq!(out.len(), 4);
    }
}
