use std::cell::Cell;
use std::sync::{Arc, Mutex};

use telar::preview::{ArgValue, Layout, PreviewCtx, PreviewEntry};
use telar::testing::{damage, find_text, mount};
use telar::{
    Children, Color, ComponentList, DrawCommand, Event, LayoutError, LayoutItem, LayoutStyle,
    PointerButton, PointerSource, Rect, RectStyle, ShapeStyle, Size, SizeDimension,
    StyledContainer, Text, TextStyle, UriOpener, WindowRoot, box_item, dismiss_depth,
    dispatch_overlays, for_each_with_matrix, relayout_if_dirty, reset_layout_runtime,
    set_uri_opener, signal, transform_clip_rect,
};
use telar_components::{ModalProps, modal};
use telar_devtools::workbench_scope;

use super::frame::{BORDER, HANDLE, STAGE_MARGIN, VIEWPORT_MIN, clamp_viewport};
use super::states::editor_uri;
use super::*;

const WIDTH: u32 = 900;
const HEIGHT: u32 = 700;
const FRAME_CHROME: f32 = 2.0 * STAGE_MARGIN + HANDLE + 2.0 * BORDER;
const SCRIM: Color = Color::rgba(0.0, 0.0, 0.0, 0.5);

thread_local! {
    static BUILDS: Cell<u32> = const { Cell::new(0) };
}

fn label(text: impl Into<String>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = text.into();
    Ok(box_item(Text::new(
        move || text.clone(),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

fn body(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    label("preview body")
}

fn mounted(entries: Vec<PreviewEntry>) -> (WorkshopState, ComponentList) {
    reset_layout_runtime();
    let state = WorkshopState::new(entries.into(), &Default::default());
    let tree = mount(
        WindowRoot::wrapping(canvas(&state).unwrap()).unwrap(),
        WIDTH,
        HEIGHT,
    );
    relayout_if_dirty();
    (state, tree)
}

fn workshop(entries: Vec<PreviewEntry>) -> (WorkshopState, ComponentList) {
    reset_layout_runtime();
    let state = WorkshopState::new(entries.into(), &Default::default());
    let chrome = workbench_scope(|| crate::shell::shell(&state, None)).unwrap();
    let tree = mount(
        WindowRoot::wrapping(box_item(chrome)).unwrap(),
        WIDTH,
        HEIGHT,
    );
    relayout_if_dirty();
    (state, tree)
}

/// A string as the window shows it: where it lands, and the clip it is drawn under.
struct Drawn {
    text: String,
    rect: Rect,
    clip: Option<Rect>,
}

fn drawn(tree: &ComponentList) -> Vec<Drawn> {
    let mut clips = Vec::new();
    let mut found = Vec::new();
    for_each_with_matrix(&tree.commands(), |command, matrix| match command {
        DrawCommand::PushClip { rect, .. } => clips.push(transform_clip_rect(matrix, *rect)),
        DrawCommand::PopClip => {
            clips.pop();
        }
        DrawCommand::Text { text, rect, .. } => found.push(Drawn {
            text: text.to_string(),
            rect: transform_clip_rect(matrix, *rect),
            clip: clips.last().copied(),
        }),
        _ => {}
    });
    found
}

fn find(tree: &ComponentList, needle: &str) -> Drawn {
    drawn(tree)
        .into_iter()
        .find(|drawn| drawn.text.contains(needle))
        .unwrap_or_else(|| panic!("{needle} is not drawn"))
}

/// Each fill of `color` the window draws, with the rect it lands on and the clip it is drawn under.
fn fills(tree: &ComponentList, color: Color) -> Vec<(Rect, Option<Rect>)> {
    let mut clips = Vec::new();
    let mut found = Vec::new();
    for_each_with_matrix(&tree.commands(), |command, matrix| match command {
        DrawCommand::PushClip { rect, .. } => clips.push(transform_clip_rect(matrix, *rect)),
        DrawCommand::PopClip => {
            clips.pop();
        }
        DrawCommand::Rect { rect, style } if style.fill == Some(color.into()) => {
            found.push((transform_clip_rect(matrix, *rect), clips.last().copied()))
        }
        _ => {}
    });
    found
}

/// The canvas's rect in the window: the clip `needle`, drawn inside it, is drawn under.
fn canvas_rect(tree: &ComponentList, needle: &str) -> Rect {
    find(tree, needle)
        .clip
        .unwrap_or_else(|| panic!("{needle} is drawn outside any canvas"))
}

fn inside(inner: Rect, outer: Rect) -> bool {
    const SLACK: f32 = 0.5;
    inner.x >= outer.x - SLACK
        && inner.y >= outer.y - SLACK
        && inner.x + inner.width <= outer.x + outer.width + SLACK
        && inner.y + inner.height <= outer.y + outer.height + SLACK
}

fn press(x: f32, y: f32) -> Event {
    Event::PointerPressed {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn release(x: f32, y: f32) -> Event {
    Event::PointerReleased {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn moved(x: f32, y: f32) -> Event {
    Event::PointerMoved {
        x: x as f64,
        y: y as f64,
        source: PointerSource::Mouse,
    }
}

/// Routes `event` as the runner does: the window's overlay layer, a live drag included, first, and the tree only if that ignores it.
fn send(tree: &ComponentList, event: Event) {
    if !dispatch_overlays(&event) {
        tree.dispatch(&event);
    }
    relayout_if_dirty();
}

fn click(tree: &ComponentList, rect: Rect) {
    let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    send(tree, press(x, y));
    send(tree, release(x, y));
}

fn drag(tree: &ComponentList, from: (f32, f32), by: (f32, f32)) {
    let to = (from.0 + by.0, from.1 + by.1);
    for event in [
        press(from.0, from.1),
        moved((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0),
        moved(to.0, to.1),
        release(to.0, to.1),
    ] {
        send(tree, event);
    }
}

fn corner_handle(frame: Rect) -> (f32, f32) {
    (
        frame.x + frame.width + BORDER + HANDLE / 2.0,
        frame.y + frame.height + BORDER + HANDLE / 2.0,
    )
}

#[test]
fn a_modal_preview_stays_inside_its_canvas() {
    let (_state, tree) = mounted(vec![
        PreviewEntry::new("fake--modal--open", "modal", "Open", |_| {
            modal(
                ModalProps::props()
                    .open(signal(true))
                    .title("Confirm")
                    .build(),
                Children::default(),
            )
        })
        .viewport(600.0, 400.0),
    ]);
    let frame = canvas_rect(&tree, "Confirm");
    assert_eq!((frame.width, frame.height), (600.0, 400.0));
    let title = find(&tree, "Confirm").rect;
    assert!(inside(title, frame), "{title:?} is outside {frame:?}");
    assert_eq!(
        fills(&tree, SCRIM),
        [(frame, Some(frame))],
        "the scrim covers the canvas, not the window, and is clipped to it"
    );
    assert_eq!(
        dismiss_depth(),
        0,
        "the window's dismiss stack knows nothing of the modal"
    );
}

struct Recorder(Arc<Mutex<Vec<String>>>);

impl UriOpener for Recorder {
    fn open(&self, uri: &str) -> bool {
        self.0.lock().unwrap().push(uri.to_owned());
        true
    }
}

#[test]
fn a_build_error_shows_a_card_naming_where_the_preview_is_written() {
    let opened = Arc::new(Mutex::new(Vec::new()));
    set_uri_opener(Arc::new(Recorder(Arc::clone(&opened))));
    let (_state, tree) = mounted(vec![
        PreviewEntry::new("fake--broken--fails", "broken", "Fails", |_| {
            Err(LayoutError::Engine("no room for the label".into()))
        })
        .location("/work/src/broken.rs", 42),
    ]);
    for text in [
        "This preview failed to build",
        "no room for the label",
        "/work/src/broken.rs:42",
        "Open in editor",
    ] {
        assert!(find_text(&tree, text), "{text} is not drawn");
    }
    click(&tree, find(&tree, "Open in editor").rect);
    assert_eq!(
        *opened.lock().unwrap(),
        ["vscode://file/work/src/broken.rs:42"]
    );
}

#[test]
fn an_editor_uri_encodes_the_path_with_forward_slashes() {
    assert_eq!(
        editor_uri("C:\\work\\my app\\button.rs", 7),
        "vscode://file/C:/work/my%20app/button.rs:7"
    );
    assert_eq!(editor_uri("/work/#1.rs", 0), "vscode://file/work/%231.rs");
}

fn fails_once(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    BUILDS.set(BUILDS.get() + 1);
    if BUILDS.get() == 1 {
        panic!("the first build always panics");
    }
    label("recovered body")
}

#[test]
fn a_panicking_preview_shows_a_boundary_card_and_remount_retries_it() {
    let (state, tree) = workshop(vec![
        PreviewEntry::new("fake--flaky--once", "flaky", "Once", fails_once),
        PreviewEntry::new("fake--steady--body", "steady", "Body", |_| {
            label("steady body")
        }),
    ]);
    for text in [
        "This preview panicked",
        "the first build always panics",
        "Workshop",
    ] {
        assert!(find_text(&tree, text), "{text} is not drawn");
    }

    let in_card = drawn(&tree)
        .into_iter()
        .rfind(|drawn| drawn.text == "Remount")
        .expect("the card offers a remount");
    click(&tree, in_card.rect);
    assert_eq!(BUILDS.get(), 2);
    assert!(find_text(&tree, "recovered body"));
    assert!(!find_text(&tree, "This preview panicked"));

    assert!(state.select("fake--steady--body"));
    relayout_if_dirty();
    assert!(find_text(&tree, "steady body"), "the workshop kept running");
}

#[test]
fn a_padded_preview_sits_at_the_top_start_of_a_canvas_its_viewport_sizes() {
    let (_state, tree) = mounted(vec![
        PreviewEntry::new("fake--box--padded", "box", "Padded", body).viewport(300.0, 200.0),
    ]);
    let frame = canvas_rect(&tree, "preview body");
    let body = find(&tree, "preview body").rect;
    assert_eq!((frame.width, frame.height), (300.0, 200.0));
    assert_eq!(
        (body.x, body.y),
        (frame.x + PAGE_PADDING, frame.y + PAGE_PADDING)
    );
    assert!(find_text(&tree, "300 × 200"));
}

#[test]
fn a_centred_preview_sits_in_the_middle_of_its_canvas() {
    let (_state, tree) = mounted(vec![
        PreviewEntry::new("fake--box--centred", "box", "Centred", body)
            .layout(Layout::Centered)
            .viewport(300.0, 200.0),
    ]);
    let frame = canvas_rect(&tree, "preview body");
    let body = find(&tree, "preview body").rect;
    let centre = |rect: Rect| (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let (body, frame) = (centre(body), centre(frame));
    assert!((body.0 - frame.0).abs() < 1.0 && (body.1 - frame.1).abs() < 1.0);
}

#[test]
fn a_fullscreen_preview_fills_the_pane_with_no_frame() {
    let (_state, tree) = mounted(vec![
        PreviewEntry::new("fake--box--fullscreen", "box", "Fullscreen", body)
            .layout(Layout::Fullscreen)
            .viewport(300.0, 200.0),
    ]);
    let frame = canvas_rect(&tree, "preview body");
    let body = find(&tree, "preview body").rect;
    assert_eq!(
        frame,
        Rect::new(
            0.0,
            HEADER_HEIGHT,
            WIDTH as f32,
            HEIGHT as f32 - HEADER_HEIGHT
        )
    );
    assert_eq!((body.x, body.y), (frame.x, frame.y));
    assert!(!find_text(&tree, " × "), "there is no frame to measure");
}

const SHELL: Color = Color::rgba(0.1, 0.4, 0.7, 1.0);

fn shell(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(StyledContainer::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        |_| RectStyle::default().with_fill(SHELL),
        vec![label("shell body")?],
    )?))
}

#[test]
fn a_fullscreen_preview_that_fills_its_parent_fills_the_whole_canvas() {
    let (_state, tree) = mounted(vec![
        PreviewEntry::new("fake--shell--fullscreen", "shell", "Fullscreen", shell)
            .layout(Layout::Fullscreen),
    ]);
    let frame = canvas_rect(&tree, "shell body");
    assert_eq!(
        fills(&tree, SHELL),
        [(frame, Some(frame))],
        "the preview fills the canvas's height as well as its width"
    );
}

#[test]
fn a_centred_preview_that_fills_its_parent_stays_centred_at_its_own_size() {
    let (_state, tree) = mounted(vec![
        PreviewEntry::new("fake--shell--centred", "shell", "Centred", shell)
            .layout(Layout::Centered)
            .viewport(300.0, 200.0),
    ]);
    let frame = canvas_rect(&tree, "shell body");
    let [(drawn, _)] = fills(&tree, SHELL)[..] else {
        panic!("the shell is drawn once");
    };
    let body = find(&tree, "shell body").rect;
    assert!(
        drawn.height < frame.height / 2.0,
        "{drawn:?} took the canvas's height rather than its own"
    );
    assert!((drawn.y + drawn.height / 2.0 - (frame.y + frame.height / 2.0)).abs() < 1.0);
    assert!(inside(body, drawn));
}

#[test]
fn a_preview_with_no_viewport_fills_the_stage_inside_the_frame() {
    let (_state, tree) = mounted(vec![PreviewEntry::new(
        "fake--box--filling",
        "box",
        "Filling",
        body,
    )]);
    let frame = canvas_rect(&tree, "preview body");
    assert_eq!(
        (frame.width, frame.height),
        (
            WIDTH as f32 - FRAME_CHROME,
            HEIGHT as f32 - HEADER_HEIGHT - FRAME_CHROME
        )
    );
}

#[test]
fn a_viewport_is_kept_within_the_minimum_and_the_room_it_has() {
    let room = Size::new(500.0, 400.0);
    assert_eq!(
        clamp_viewport(Size::new(10.0, 9000.0), room),
        Size::new(VIEWPORT_MIN, 400.0)
    );
    assert_eq!(
        clamp_viewport(Size::new(320.0, 240.0), room),
        Size::new(320.0, 240.0)
    );
    assert_eq!(
        clamp_viewport(Size::new(320.0, 240.0), Size::ZERO),
        Size::new(VIEWPORT_MIN, VIEWPORT_MIN)
    );
}

#[test]
fn dragging_the_corner_handle_resizes_the_viewport_within_the_stage() {
    let (state, tree) = mounted(vec![PreviewEntry::new(
        "fake--box--resized",
        "box",
        "Resized",
        body,
    )]);
    let room = (
        WIDTH as f32 - FRAME_CHROME,
        HEIGHT as f32 - HEADER_HEIGHT - FRAME_CHROME,
    );

    drag(
        &tree,
        corner_handle(canvas_rect(&tree, "preview body")),
        (-5000.0, -5000.0),
    );
    assert_eq!(state.viewport().get(), Some((VIEWPORT_MIN, VIEWPORT_MIN)));
    let frame = canvas_rect(&tree, "preview body");
    assert_eq!((frame.width, frame.height), (VIEWPORT_MIN, VIEWPORT_MIN));
    assert!(find_text(&tree, "32 × 32"));

    drag(&tree, corner_handle(frame), (5000.0, 5000.0));
    assert_eq!(state.viewport().get(), Some(room));
}

fn counted(ctx: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    BUILDS.set(BUILDS.get() + 1);
    label(ctx.arg("label", "first body".to_string()))
}

#[test]
fn an_arg_change_builds_the_preview_again_inside_the_same_canvas() {
    let (state, tree) = mounted(vec![
        PreviewEntry::new("fake--counted--body", "counted", "Body", counted).viewport(300.0, 200.0),
    ]);
    let frame = canvas_rect(&tree, "first body");
    let before = tree.commands().clone();

    state
        .args(&state.entries()[0])
        .set("label", ArgValue::Text("second body".into()))
        .unwrap();
    relayout_if_dirty();

    assert!(find_text(&tree, "second body"));
    assert_eq!(BUILDS.get(), 2);
    let framed = Rect::new(
        frame.x - BORDER,
        frame.y - BORDER,
        frame.width + 2.0 * BORDER + HANDLE,
        frame.height + 2.0 * BORDER + HANDLE,
    );
    let repainted = damage(&tree.commands(), &before)
        .expect("the change repaints part of the window, not all of it");
    assert!(
        repainted.iter().all(|rect| inside(*rect, framed)),
        "only the viewport frame repaints: {repainted:?} reaches outside {framed:?}"
    );
}
