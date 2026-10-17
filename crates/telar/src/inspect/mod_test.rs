use std::rc::Rc;

use super::*;
use crate::{
    AccessRule, AvailableSpace, Color, ComponentList, Container, LayoutError, LayoutItem,
    LayoutStyle, Rect, RectStyle, Role, ShapeStyle, SizeDimension, StyledContainer, SurfaceCanvas,
    SurfaceFrame, Text, TextStyle, WindowRoot, box_item, compute_layout, relayout_if_dirty,
    transform_clip_rect,
};

const WINDOW: f32 = 400.0;

fn fill() -> LayoutStyle {
    LayoutStyle::new()
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
}

fn button(label: Option<&'static str>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let children = label
        .map(|label| {
            Text::new(
                move || label.to_string(),
                LayoutStyle::new().width(48.0).height(16.0),
                || TextStyle::new(12.0, Color::BLACK),
            )
            .map(box_item)
        })
        .transpose()?
        .into_iter()
        .collect();
    Ok(box_item(
        StyledContainer::new(
            LayoutStyle::new().width(64.0).height(32.0),
            |_| RectStyle::default().with_fill(Color::WHITE),
            children,
        )?
        .control(Role::Button)
        .on_press(|| {}),
    ))
}

fn canvas() -> Rc<SurfaceCanvas> {
    Rc::new(
        SurfaceCanvas::new(crate::Size::new(1.0, 1.0), || {
            let page = StyledContainer::new(
                fill().flex_column().gap(8.0).padding_all(8.0),
                |_| RectStyle::default().with_fill(Color::WHITE),
                vec![button(Some("Inside"))?, button(None)?],
            )?;
            Ok(box_item(page))
        })
        .unwrap(),
    )
}

/// A window painted `backdrop`, with `canvas` shown in a frame 200 × 120 at (40, 60), beside a button of its own.
fn window(canvas: &Rc<SurfaceCanvas>, backdrop: Color) -> ComponentList {
    let spacer = |width: f32, height: f32| {
        box_item(Container::new(LayoutStyle::new().width(width).height(height), vec![]).unwrap())
    };
    let frame = SurfaceFrame::filling(
        Rc::clone(canvas),
        LayoutStyle::new().width(200.0).height(120.0),
    )
    .unwrap();
    let row = Container::new(
        LayoutStyle::new().flex_row().height(120.0),
        vec![spacer(40.0, 120.0), box_item(frame)],
    )
    .unwrap();
    let content = StyledContainer::new(
        fill().flex_column(),
        move |_| RectStyle::default().with_fill(backdrop),
        vec![
            spacer(10.0, 60.0),
            box_item(row),
            button(Some("Outside")).unwrap(),
        ],
    )
    .unwrap();
    let root = content.layout_node();
    crate::set_surface_size(crate::Size::new(WINDOW, WINDOW));
    let tree = ComponentList::new(WindowRoot::new(box_item(content)));
    compute_layout(
        root,
        AvailableSpace::Definite(WINDOW),
        AvailableSpace::Definite(WINDOW),
    )
    .unwrap();
    relayout_if_dirty();
    tree
}

fn placed(canvas: &SurfaceCanvas, rect: Rect) -> Rect {
    transform_clip_rect(canvas.placement().to_array(), rect)
}

#[test]
fn the_window_and_the_canvas_agree_on_what_a_reader_is_told_inside_the_canvas() {
    crate::install_default_text_metrics();
    crate::reset_layout_runtime();
    crate::focus::clear();
    let canvas = canvas();
    let tree = window(&canvas, Color::WHITE);
    let window_frame = tree.commands().clone();
    let whole = audit(&window_frame, None);

    let own = {
        let commands = canvas.frame_commands().clone();
        let _entered = canvas.enter();
        audit(&commands, None)
    };
    assert!(
        own.nodes.iter().any(|node| node.name == "Inside"),
        "{:#?}",
        own.nodes
    );
    for node in &own.nodes {
        let rect = placed(&canvas, node.rect);
        assert!(
            whole
                .nodes
                .iter()
                .any(|seen| seen.name == node.name && seen.role == node.role && seen.rect == rect),
            "the window tells a reader of {:?} where the frame shows it: {:#?}",
            node.name,
            whole.nodes
        );
    }
    assert!(
        whole.nodes.iter().any(|node| node.name == "Outside"),
        "and of its own controls"
    );

    let unnamed: Vec<Rect> = own
        .violations
        .iter()
        .filter(|violation| violation.rule == AccessRule::UnnamedControl)
        .filter_map(|violation| violation.rect)
        .collect();
    assert_eq!(unnamed.len(), 1, "the canvas finds its unnamed button");
    let found_in_window: Vec<Rect> = whole
        .violations
        .iter()
        .filter(|violation| violation.rule == AccessRule::UnnamedControl)
        .filter_map(|violation| violation.rect)
        .collect();
    assert_eq!(
        found_in_window,
        vec![placed(&canvas, unnamed[0])],
        "the window finds the same one, once, where the frame shows it"
    );
}

#[test]
fn a_surface_nobody_can_enter_any_more_is_left_out_rather_than_read_in_the_wrong_world() {
    crate::reset_layout_runtime();
    crate::focus::clear();
    let commands = vec![
        crate::DrawCommand::PushSurface { surface: u64::MAX },
        crate::DrawCommand::Text {
            text: "Gone".into(),
            spans: None,
            rect: Rect::new(0.0, 0.0, 40.0, 16.0),
            style: std::sync::Arc::new(TextStyle::new(12.0, Color::BLACK)),
        },
        crate::DrawCommand::PopSurface,
    ];
    let read = audit(&commands, None);
    assert!(read.nodes.is_empty(), "{:#?}", read.nodes);
    assert!(read.violations.is_empty());
}

#[test]
fn text_a_canvas_draws_on_nothing_of_its_own_is_judged_against_what_the_window_shows_behind_it() {
    crate::install_default_text_metrics();
    crate::reset_layout_runtime();
    crate::focus::clear();
    let canvas = Rc::new(
        SurfaceCanvas::new(crate::Size::new(1.0, 1.0), || {
            Ok(box_item(Text::new(
                || "Unreadable".to_string(),
                LayoutStyle::new().width(80.0).height(16.0),
                || TextStyle::new(12.0, Color::BLACK),
            )?))
        })
        .unwrap(),
    );
    let tree = window(&canvas, Color::BLACK);
    let read = audit(&tree.commands().clone(), Some(Color::WHITE));
    assert!(
        read.violations
            .iter()
            .any(|violation| violation.rule == AccessRule::Contrast),
        "{:#?}",
        read.violations
    );
}
