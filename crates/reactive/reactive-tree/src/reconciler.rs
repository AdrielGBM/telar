use renderer_core::DrawCommand;

use crate::view::View;

pub(crate) fn flatten(root: View) -> Vec<DrawCommand> {
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
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use renderer_core::{Color, Rect, RectStyle};

    use super::*;

    fn sample_rect() -> DrawCommand {
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            style: RectStyle::default().with_fill(Color::BLACK),
        }
    }

    #[test]
    fn flatten_empty_returns_empty() {
        let out = flatten(View::Empty);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_group_of_empties() {
        let view = View::Group(vec![View::Empty, View::Empty, View::Empty]);
        let out = flatten(view);
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
        let out = flatten(view);
        assert_eq!(out.len(), 4);
    }
}
