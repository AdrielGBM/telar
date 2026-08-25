//! What a screen reader is told about the window right now.
//!
//! Telar had the whole of *operating* an interface without a mouse — a tab order scoped to what is genuinely
//! reachable, arrow keys and type-ahead in every list, a focus ring that knows a tap from a Tab — and none of
//! it reaches someone who cannot see the screen. That is a different question: not *where do the keys go* but
//! *what is this and what state is it in*.
//!
//! Nothing here is authored per widget, and that is the design. The set of controls is the focus registry's
//! own tab order, so the reader and the keyboard cannot come to different conclusions about what is on screen.
//! The name is taken from **the text the widget actually draws inside itself**, which is how it reads to
//! everyone else and needs no second copy that can fall out of step with the first. Only the role is declared,
//! and only where the default of "a thing you activate" is wrong.

use geometry_core::Rect;
use platform_core::{AccessNode, Role};
use renderer_core::DrawCommand;

use crate::focus;

/// Everything the platform's accessibility layer needs to describe the window, in reading order.
///
/// `commands` is the frame the renderer is about to draw — the same list, so what is announced and what is
/// painted are the same picture by construction rather than by agreement.
pub fn snapshot(commands: &[DrawCommand]) -> Vec<AccessNode> {
    let controls: Vec<(focus::Exposed, Rect)> = focus::exposed()
        .into_iter()
        .filter_map(|e| layout_reactive::absolute_rect(e.node).map(|rect| (e, rect)))
        .collect();
    let focused = focus::current();

    let mut nodes: Vec<AccessNode> = controls
        .iter()
        .map(|(e, rect)| AccessNode {
            id: Some(e.id),
            role: e.role,
            name: String::new(),
            rect: *rect,
            focused: focused == Some(e.id),
            enabled: e.enabled,
            toggled: e.toggled,
            value: e.value,
        })
        .collect();

    for (text, rect) in drawn_text(commands) {
        // The smallest control containing it, so a button inside a card is named by its own label rather than
        // by everything the card happens to hold.
        let owner = controls
            .iter()
            .enumerate()
            .filter(|(_, (_, bounds))| contains(*bounds, rect))
            .min_by(|(_, (_, a)), (_, (_, b))| area(*a).total_cmp(&area(*b)));
        match owner {
            Some((i, _)) => append(&mut nodes[i].name, &text),
            // Text belonging to no control is still content: a heading, a caption, the paragraph a dialog is
            // asking about. A reader given only the buttons cannot tell you what the buttons are for.
            None => nodes.push(AccessNode {
                id: None,
                role: Role::Label,
                name: text,
                rect,
                focused: false,
                enabled: true,
                toggled: None,
                value: None,
            }),
        }
    }

    // Reading order rather than tab order: a label sits between the controls it explains, and it has no place
    // in a list built from registration. Tab order stays the focus registry's answer, where it belongs.
    nodes.sort_by(|a, b| {
        a.rect
            .y
            .total_cmp(&b.rect.y)
            .then(a.rect.x.total_cmp(&b.rect.x))
    });
    nodes.retain(|n| n.id.is_some() || !n.name.is_empty());
    nodes
}

/// Every string the frame draws, with where it draws it.
fn drawn_text(commands: &[DrawCommand]) -> Vec<(String, Rect)> {
    commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, rect, .. } => Some((text.to_string(), *rect)),
            _ => None,
        })
        .filter(|(text, _)| !text.trim().is_empty())
        .collect()
}

/// Whether `inner`'s centre lies in `outer`. The centre and not the whole rect: a label clipped by its own
/// control — a long menu item, a cell in a narrow column — still belongs to it.
fn contains(outer: Rect, inner: Rect) -> bool {
    let (x, y) = (inner.x + inner.width / 2.0, inner.y + inner.height / 2.0);
    x >= outer.x && x <= outer.x + outer.width && y >= outer.y && y <= outer.y + outer.height
}

fn area(rect: Rect) -> f32 {
    rect.width * rect.height
}

fn append(name: &mut String, piece: &str) {
    if !name.is_empty() {
        name.push(' ');
    }
    name.push_str(piece.trim());
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use layout_core::{AvailableSpace, LayoutStyle};
    use renderer_core::{RectStyle, TextStyle};

    use super::*;
    use crate::container::Container;
    use crate::context::{compute_layout, reset_layout_runtime};
    use crate::layout_item::{LayoutItem, box_item};
    use crate::styled_container::StyledContainer;
    use crate::text::Text;

    fn text_at(text: &str, rect: Rect) -> DrawCommand {
        DrawCommand::Text {
            spans: None,
            text: Arc::from(text),
            rect,
            style: Arc::new(TextStyle::new(12.0, renderer_core::Color::BLACK)),
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x, y, w, h)
    }

    /// A button built the ordinary way is announced with the label it draws — nobody wrote an accessible name
    /// for it, and that is the point: a second copy of the text is a second thing to keep true.
    #[test]
    fn a_control_is_named_by_the_text_it_draws() {
        reset_layout_runtime();
        focus::clear();
        let label = Text::new(
            || "Save".to_string(),
            LayoutStyle::new().width(40.0).height(16.0),
            || TextStyle::new(12.0, renderer_core::Color::BLACK),
        )
        .unwrap();
        let button = StyledContainer::new(
            LayoutStyle::new().width(80.0).height(30.0),
            |_r| RectStyle::default(),
            vec![box_item(label)],
        )
        .unwrap()
        .control(Role::Button)
        .on_press(|| {});
        let root =
            Container::new(LayoutStyle::new().flex_column(), vec![box_item(button)]).unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let nodes = snapshot(&[text_at("Save", rect(10.0, 5.0, 40.0, 16.0))]);
        let button = nodes
            .iter()
            .find(|n| n.id.is_some())
            .expect("the button is exposed");
        assert_eq!(button.name, "Save");
        assert_eq!(button.role, Role::Button);
        assert!(button.enabled);
    }

    /// Text belonging to no control is content, not noise. A reader handed only the buttons cannot say what
    /// the buttons are for.
    #[test]
    fn text_outside_any_control_is_still_announced() {
        reset_layout_runtime();
        focus::clear();
        let nodes = snapshot(&[text_at("Delete everything?", rect(0.0, 0.0, 200.0, 20.0))]);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].role, Role::Label);
        assert_eq!(nodes[0].name, "Delete everything?");
        assert_eq!(nodes[0].id, None);
    }

    /// Nesting: the label goes to the smallest control that contains it, so a button inside a card is named by
    /// its own text rather than by everything the card happens to hold.
    #[test]
    fn a_label_belongs_to_the_smallest_control_around_it() {
        let outer = rect(0.0, 0.0, 200.0, 100.0);
        let inner = rect(10.0, 10.0, 50.0, 20.0);
        assert!(contains(outer, inner));
        assert!(contains(inner, rect(20.0, 15.0, 10.0, 8.0)));
        assert!(!contains(inner, rect(120.0, 15.0, 10.0, 8.0)));
        assert!(area(inner) < area(outer));
    }

    /// A value control that reports only its role has not said the one thing it exists to report: a slider
    /// announced as "Volume, slider" leaves the number — the whole content of the control — unsaid.
    #[test]
    fn a_valued_control_reports_where_it_stands() {
        reset_layout_runtime();
        focus::clear();
        let track = StyledContainer::new(
            LayoutStyle::new().width(200.0).height(20.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .control(Role::Slider)
        .valued(|| platform_core::NumericValue {
            now: 0.25,
            min: 0.0,
            max: 1.0,
        });
        let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(track)]).unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let nodes = snapshot(&[]);
        let slider = nodes.iter().find(|n| n.id.is_some()).expect("exposed");
        let value = slider.value.expect("a slider carries a number");
        assert_eq!(value.now, 0.25);
        assert_eq!((value.min, value.max), (0.0, 1.0));
    }

    /// `toggled` was populated only through `labelled_control`, so a tab never said it was the selected one.
    #[test]
    fn a_control_that_carries_a_state_reports_it() {
        reset_layout_runtime();
        focus::clear();
        let tab = StyledContainer::new(
            LayoutStyle::new().width(80.0).height(30.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .control(Role::Tab)
        .toggled(|| true);
        let root = Container::new(LayoutStyle::new().flex_column(), vec![box_item(tab)]).unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(200.0),
        )
        .unwrap();

        let nodes = snapshot(&[]);
        let tab = nodes.iter().find(|n| n.id.is_some()).expect("exposed");
        assert_eq!(tab.toggled, Some(true));
    }

    /// The reading order a reader walks is the order things sit on screen, not the order they were built —
    /// which is what tab order is, and stays.
    #[test]
    fn nodes_come_back_in_reading_order() {
        reset_layout_runtime();
        focus::clear();
        let nodes = snapshot(&[
            text_at("second", rect(0.0, 40.0, 50.0, 16.0)),
            text_at("first", rect(0.0, 10.0, 50.0, 16.0)),
            text_at("third", rect(60.0, 40.0, 50.0, 16.0)),
        ]);
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }
}
