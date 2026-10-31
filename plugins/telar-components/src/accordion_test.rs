use telar::testing::{press, release};
use telar::{AvailableSpace, compute_layout, new_container, track_layout};

use super::*;
use crate::test_support::text_body;

#[test]
fn pressing_the_header_toggles_open() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let mut item = accordion(
        AccordionProps::props()
            .title("Details")
            .open(open)
            .children(text_body("Body"))
            .build(),
    )
    .unwrap();
    let node = item.layout_node();
    let rect = track_layout(node).unwrap();
    compute_layout(
        node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let r = rect.get();
    // The header is the top row, so a point near the top-left lands on it whatever the collapsed body's height.
    let (cx, cy) = ((r.x + 10.0) as f64, (r.y + 5.0) as f64);

    item.on_event(&press(cx, cy));
    item.on_event(&release(cx, cy));
    assert!(open.get(), "a tap on the header opens the section");

    item.on_event(&press(cx, cy));
    item.on_event(&release(cx, cy));
    assert!(!open.get(), "a second tap closes it again");
}

// Laid out as a child of a fixed-size wrapper, not as the layout root: an auto-height root fills its available space, which would stretch the accordion to 400px both closed and open and hide the difference.
#[test]
fn open_expands_body_and_close_collapses_it() {
    crate::test_support::fresh_layout_runtime();
    let open = signal(false);
    let item = accordion(
        AccordionProps::props()
            .title("Details")
            .open(open)
            .children(text_body("Body"))
            .build(),
    )
    .unwrap();
    let node = item.layout_node();
    let root_rect = track_layout(node).unwrap();
    let wrapper = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[node],
    )
    .unwrap();
    compute_layout(
        wrapper,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let closed_height = root_rect.get().height;

    open.set(true);
    compute_layout(
        wrapper,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let open_height = root_rect.get().height;

    assert!(
        open_height > closed_height,
        "opening the section grows the root's height (closed: {closed_height}, open: {open_height})"
    );
}

#[test]
fn uncontrolled_accordion_builds_and_starts_closed() {
    crate::test_support::fresh_layout_runtime();
    let item = accordion(
        AccordionProps::props()
            .title("Details")
            .children(text_body("Body"))
            .build(),
    )
    .unwrap();
    let node = item.layout_node();
    let root_rect = track_layout(node).unwrap();
    let wrapper = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[node],
    )
    .unwrap();
    compute_layout(
        wrapper,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let height = root_rect.get().height;
    assert!(
        height > 0.0 && height <= 55.0,
        "closed, only the header should contribute height, got {height}"
    );
}

#[derive(Clone, Default)]
struct Plain {
    accordions: Option<AccordionTokens>,
}

impl telar::ThemeTokens for Plain {
    fn radius_control(&self) -> f32 {
        6.0
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(accordions) = &self.accordions {
            extensions.insert(accordions.clone());
        }
    }
}

fn header_paint(style: Option<SurfaceAmendment>, hovered: bool) -> (telar::Rect, f32, RectStyle) {
    use telar::testing::{centre, moved, route};

    crate::test_support::fresh_layout_runtime();
    let mut tree = telar::testing::scoped(|| {
        let mut props = AccordionProps::props().title("Details").build();
        props.style = style;
        let item = accordion(AccordionProps {
            children: text_body("Body"),
            ..props
        })
        .unwrap();
        telar::testing::lay_out(item.layout_node(), 400.0, 400.0);
        telar::ComponentList::new(item)
    });
    let header = crate::test_support::drawn(Role::Disclosure)[0];
    if hovered {
        let (x, y) = centre(header);
        route(&mut tree, &moved(x, y));
    }
    let commands = tree.commands();
    let style = commands
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Rect { rect, style } if *rect == header => Some(**style),
            _ => None,
        })
        .unwrap_or_default();
    let mut shift = 0.0;
    let title = commands
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::PushMatrix { matrix } => {
                shift = matrix[4];
                None
            }
            telar::DrawCommand::Text { rect, .. } => Some(rect.x + shift),
            _ => None,
        })
        .expect("the title is drawn");
    (header, title, style)
}

#[test]
fn the_header_is_a_row_tall_and_padded_like_a_control() {
    telar::set_theme(Plain::default());
    let (header, title, _) = header_paint(None, false);
    assert!(header.height >= crate::metrics::row_height());
    assert_eq!(title - header.x, crate::metrics::control_pad_x());

    telar::set_theme(Plain {
        accordions: Some(AccordionTokens {
            pad_x: Some(20.0),
            radius: None,
        }),
    });
    let (header, title, _) = header_paint(None, false);
    assert_eq!(title - header.x, 20.0);
}

#[test]
fn a_hovered_header_takes_the_theme_s_wash_on_the_amended_shape() {
    telar::set_theme(Plain::default());
    let (_, _, rest) = header_paint(None, false);
    assert_eq!(rest.radius, telar::BorderRadius::all(6.0));
    let (_, _, hovered) = header_paint(None, true);
    assert_eq!(
        hovered.fill.map(|f| f.solid_color()),
        Some(crate::metrics::hover_wash())
    );

    let square: SurfaceAmendment =
        std::rc::Rc::new(|s: RectStyle| s.with_radius(telar::BorderRadius::all(0.0)));
    let (_, _, hovered) = header_paint(Some(square), true);
    assert!(hovered.fill.is_some());
    assert_eq!(hovered.radius, telar::BorderRadius::all(0.0));
}
