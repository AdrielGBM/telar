use renderer_core::DrawCommand;

use crate::view::View;

pub fn flatten_view(root: View) -> Vec<DrawCommand> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(view) = stack.pop() {
        match view {
            View::Empty => {}
            View::Primitive(cmd) => out.push(cmd),
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
                out.push(DrawCommand::PushTransform { tx, ty });
            }
            View::Clip { rect, children } => {
                stack.push(View::Primitive(DrawCommand::PopClip));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                out.push(DrawCommand::PushClip { rect });
            }
            View::Layer { opacity, children } => {
                stack.push(View::Primitive(DrawCommand::PopLayer));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                out.push(DrawCommand::PushLayer { opacity });
            }
        }
    }
    out
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
        let out = flatten_view(View::Empty);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_group_of_empties() {
        let view = View::Group(vec![View::Empty, View::Empty, View::Empty]);
        let out = flatten_view(view);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_nested_groups() {
        let view = View::Group(vec![
            View::Primitive(sample_rect()),
            View::Group(vec![
                View::Primitive(sample_rect()),
                View::Empty,
                View::Group(vec![View::Primitive(sample_rect())]),
            ]),
            View::Primitive(sample_rect()),
        ]);
        let out = flatten_view(view);
        assert_eq!(out.len(), 4);
    }
}
