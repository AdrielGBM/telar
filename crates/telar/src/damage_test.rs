//! What `testing::damage` says a change repaints: the box that changed and nothing beside it, nothing for a frame that is the same, and what a box covered where its paint went away.

use telar::testing::{damage, mount};
use telar::{
    Color, Container, DrawCommand, LayoutItem, LayoutStyle, Rect, RectStyle, StyledContainer,
    WindowRoot, box_item, reset_layout_runtime, signal,
};

fn tile(x: f32, fill: impl Fn() -> Color + 'static) -> Box<dyn LayoutItem> {
    box_item(
        StyledContainer::new(
            LayoutStyle::new()
                .absolute()
                .inset_start(x)
                .inset_top(0.0)
                .width(100.0)
                .height(80.0),
            move |_| RectStyle::filled(fill(), 0.0),
            vec![],
        )
        .expect("a tile"),
    )
}

fn page(tiles: Vec<Box<dyn LayoutItem>>) -> WindowRoot {
    WindowRoot::new(box_item(
        Container::new(LayoutStyle::new().width(300.0).height(80.0), tiles).expect("a page"),
    ))
}

fn frame(tree: &ui_core::ComponentList) -> Vec<DrawCommand> {
    telar::relayout_if_dirty();
    tree.commands().to_vec()
}

fn within(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

fn covers(damaged: &[Rect], wanted: Rect) -> bool {
    damaged.iter().any(|rect| within(wanted, *rect))
}

#[test]
fn a_change_to_one_box_damages_that_box_and_nothing_beside_it() {
    reset_layout_runtime();
    let left = signal(Color::BLACK);
    let tree = mount(
        page(vec![
            tile(0.0, move || left.get()),
            tile(200.0, || Color::WHITE),
        ]),
        300,
        80,
    );
    let before = frame(&tree);
    assert_eq!(
        damage(&before, &before),
        Some(Vec::new()),
        "the same frame repaints nothing"
    );

    left.set(Color::from_rgb_u8(200, 30, 30));
    let after = frame(&tree);
    let damaged = damage(&after, &before).expect("a change a region bounds");
    let own = Rect::new(0.0, 0.0, 100.0, 80.0);
    assert!(covers(&damaged, own), "the box is repainted: {damaged:?}");
    assert!(
        damaged.iter().all(|rect| within(*rect, own)),
        "and nothing past it: {damaged:?}"
    );
}

#[test]
fn a_box_whose_paint_goes_away_damages_what_it_covered() {
    reset_layout_runtime();
    let shown = signal(true);
    let tree = mount(
        page(vec![
            tile(0.0, || Color::WHITE),
            tile(200.0, move || match shown.get() {
                true => Color::BLACK,
                false => Color::TRANSPARENT,
            }),
        ]),
        300,
        80,
    );
    let before = frame(&tree);
    shown.set(false);
    let after = frame(&tree);
    let damaged = damage(&after, &before).expect("a change a region bounds");
    let covered = Rect::new(200.0, 0.0, 100.0, 80.0);
    assert!(
        covers(&damaged, covered),
        "what the box covered is repainted: {damaged:?}"
    );
    assert!(
        damaged.iter().all(|rect| within(*rect, covered)),
        "and nothing else is: {damaged:?}"
    );
}
