use super::*;
use telar::testing::press;
use telar::{AvailableSpace, WindowCommand, compute_layout, take_window_commands};

const WIDTH: f32 = 400.0;
const HEIGHT: f32 = 300.0;

fn window() -> Box<dyn LayoutItem> {
    let content = StyledContainer::new(
        LayoutStyle::new().flex_grow(1.0),
        |_| RectStyle::default(),
        Vec::new(),
    )
    .unwrap();
    let root = box_item(
        StyledContainer::new(
            LayoutStyle::new().flex_column().width(WIDTH).height(HEIGHT),
            |_| RectStyle::default(),
            vec![box_item(content), window_edges().unwrap()],
        )
        .unwrap(),
    );
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(WIDTH),
        AvailableSpace::Definite(HEIGHT),
    )
    .unwrap();
    root
}

fn resized_by(x: f32, y: f32) -> Vec<ResizeEdge> {
    crate::test_support::fresh_layout_runtime();
    let mut window = window();
    take_window_commands();
    window.on_event(&press(x, y));
    take_window_commands()
        .into_iter()
        .filter_map(|command| match command {
            WindowCommand::DragResize(edge) => Some(edge),
            _ => None,
        })
        .collect()
}

#[test]
fn a_press_on_each_border_pulls_that_edge_of_the_window() {
    let (right, bottom) = (WIDTH - 1.0, HEIGHT - 1.0);
    let (middle_x, middle_y) = (WIDTH / 2.0, HEIGHT / 2.0);
    for (x, y, edge) in [
        (middle_x, 1.0, ResizeEdge::North),
        (middle_x, bottom, ResizeEdge::South),
        (1.0, middle_y, ResizeEdge::West),
        (right, middle_y, ResizeEdge::East),
        (1.0, 1.0, ResizeEdge::NorthWest),
        (right, 1.0, ResizeEdge::NorthEast),
        (1.0, bottom, ResizeEdge::SouthWest),
        (right, bottom, ResizeEdge::SouthEast),
        (CORNER - 1.0, 1.0, ResizeEdge::NorthWest),
    ] {
        assert_eq!(resized_by(x, y), vec![edge], "a press at ({x}, {y})");
    }
}

#[test]
fn a_press_inside_the_border_resizes_nothing() {
    assert!(resized_by(WIDTH / 2.0, HEIGHT / 2.0).is_empty());
    assert!(resized_by(REACH + 1.0, HEIGHT / 2.0).is_empty());
}

#[test]
fn a_right_to_left_line_puts_the_start_handles_on_the_east_edge() {
    assert_eq!(as_drawn(ResizeEdge::West, Direction::Rtl), ResizeEdge::East);
    assert_eq!(
        as_drawn(ResizeEdge::SouthEast, Direction::Rtl),
        ResizeEdge::SouthWest
    );
    assert_eq!(
        as_drawn(ResizeEdge::North, Direction::Rtl),
        ResizeEdge::North
    );
    for edge in ResizeEdge::ALL {
        assert_eq!(as_drawn(edge, Direction::Ltr), edge);
    }
}
