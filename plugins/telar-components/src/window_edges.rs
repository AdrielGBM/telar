//! [`window_edges`]: the handles a window drawn without decorations is resized by.

use telar::{
    Container, Direction, LayoutError, LayoutItem, LayoutStyle, Reactive, RectStyle, ResizeEdge,
    StyledContainer, box_item, current_direction,
};

/// How far in from the window's border an edge handle reaches.
const REACH: f32 = 4.0;
/// How far a corner handle reaches along each side it joins, so a corner is easier to catch than the edges either side of it.
const CORNER: f32 = 12.0;

/// Handles along every edge and corner of the window, each asking the platform for an interactive resize from where it is pressed, with the pointer shape that says which way it pulls.
///
/// For a window opened without decorations, which the window system then gives no border to resize it by. Place it last among the children of the window's root, so it lies over everything the root holds; outside the handles the pointer reaches whatever is under them.
pub fn window_edges() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let handles = ResizeEdge::ALL
        .into_iter()
        .map(handle)
        .collect::<Result<Vec<_>, _>>()?;
    let layer = Container::new(LayoutStyle::new().absolute_fill(), handles)?.input_transparent();
    Ok(box_item(layer))
}

fn handle(edge: ResizeEdge) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let handle = StyledContainer::new(placed(edge), |_| RectStyle::default(), Vec::new())?
        .cursor(Reactive::of(move || {
            as_drawn(edge, current_direction()).cursor()
        }))
        .on_drag(move |_, _| telar::window::drag_resize(as_drawn(edge, current_direction())));
    Ok(box_item(handle))
}

/// Where the handle for `edge` sits, with west and east written as the start and end of a left-to-right line; [`as_drawn`] says which edge it is under the direction actually laid out.
fn placed(edge: ResizeEdge) -> LayoutStyle {
    let style = LayoutStyle::new().absolute();
    let style = match edge {
        ResizeEdge::North | ResizeEdge::NorthWest | ResizeEdge::NorthEast => style.inset_top(0.0),
        ResizeEdge::South | ResizeEdge::SouthWest | ResizeEdge::SouthEast => {
            style.inset_bottom(0.0)
        }
        ResizeEdge::West | ResizeEdge::East => style.inset_top(CORNER).inset_bottom(CORNER),
    };
    let style = match edge {
        ResizeEdge::West | ResizeEdge::NorthWest | ResizeEdge::SouthWest => style.inset_start(0.0),
        ResizeEdge::East | ResizeEdge::NorthEast | ResizeEdge::SouthEast => style.inset_end(0.0),
        ResizeEdge::North | ResizeEdge::South => style.inset_start(CORNER).inset_end(CORNER),
    };
    match edge {
        ResizeEdge::North | ResizeEdge::South => style.height(REACH),
        ResizeEdge::West | ResizeEdge::East => style.width(REACH),
        _ => style.width(CORNER).height(CORNER),
    }
}

/// The edge of the window the handle [`placed`] for `edge` lies on when the line runs `direction`.
fn as_drawn(edge: ResizeEdge, direction: Direction) -> ResizeEdge {
    if direction != Direction::Rtl {
        return edge;
    }
    match edge {
        ResizeEdge::West => ResizeEdge::East,
        ResizeEdge::East => ResizeEdge::West,
        ResizeEdge::NorthWest => ResizeEdge::NorthEast,
        ResizeEdge::NorthEast => ResizeEdge::NorthWest,
        ResizeEdge::SouthWest => ResizeEdge::SouthEast,
        ResizeEdge::SouthEast => ResizeEdge::SouthWest,
        ResizeEdge::North | ResizeEdge::South => edge,
    }
}

#[cfg(test)]
#[path = "window_edges_test.rs"]
mod tests;
