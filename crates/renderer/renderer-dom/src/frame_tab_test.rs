//! Tab through a surface nested in the frame, on a document: the browser walks its own Tab order, which runs from the window's control before the frame through the controls inside it and on to the window's control after it, with no stop for the frame itself, as Telar's own Tab does on a desktop. Each step reaches the app as focus on a box, in the window or inside the frame's surface, and the app's focus follows it there.
//!
//! A synthetic Tab is untrusted, so the browser walks nothing for it: the walk is taken one step at a time by focusing the next element in the document's sequential focus order, which is what the browser's Tab does. Needs a browser, so it runs under `wasm-bindgen-test` rather than `cargo test`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use geometry_core::Size;
use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Event, Role};
use reactive_core::{RwSignal, signal};
use renderer_core::{Color, RectStyle, RenderBackend};
use telar_renderer_dom::{CanvasTextMetrics, DomRenderer, ID_ATTRIBUTE};
use ui_core::{
    ComponentList, Container, LayoutItem, StyledContainer, SurfaceCanvas, SurfaceFrame, WindowRoot,
    box_item,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
}

static REPORTED: Mutex<Vec<Event>> = Mutex::new(Vec::new());

/// Each control by the name its element is written with, beside whether the app's focus is on it.
struct Stop {
    written: String,
    focused: RwSignal<bool>,
}

struct Page {
    tree: ComponentList,
    renderer: DomRenderer,
    host: web_sys::HtmlElement,
    canvas: Rc<SurfaceCanvas>,
    frame: String,
    before: Stop,
    inside: [Stop; 2],
    after: Stop,
}

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|window| window.document())
        .expect("a document")
}

fn button(focused: RwSignal<bool>) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(60.0).height(24.0),
        |_| RectStyle::filled(Color::from_rgb_u8(40, 90, 200), 0.0),
        vec![],
    )
    .expect("the button builds")
    .control(Role::Button)
    .on_press(|| {})
    .on_focus(move |now| focused.set(now))
}

fn stop(control: &StyledContainer, surface: Option<u64>, focused: RwSignal<bool>) -> Stop {
    let id: u64 = control.layout_node().into();
    Stop {
        written: match surface {
            Some(surface) => format!("{surface}-{id}"),
            None => id.to_string(),
        },
        focused,
    }
}

fn mount() {
    if PAGE.with(|page| page.borrow().is_some()) {
        return;
    }
    renderer_core::set_text_metrics(CanvasTextMetrics);
    ui_tree::set_element_capture(true);
    ui_core::set_surface_size(Size::new(400.0, 300.0));
    platform_core::set_event_sink(Arc::new(|event| REPORTED.lock().unwrap().push(event)));

    let inside_focus = [signal(false), signal(false)];
    let built: Rc<RefCell<Vec<u64>>> = Rc::default();
    let record = Rc::clone(&built);
    let canvas = Rc::new(
        SurfaceCanvas::new(Size::new(200.0, 100.0), move || {
            let controls = inside_focus.map(button);
            record.borrow_mut().extend(
                controls
                    .iter()
                    .map(|control| u64::from(control.layout_node())),
            );
            Ok(box_item(Container::new(
                LayoutStyle::new().flex_row().gap(8.0),
                controls
                    .into_iter()
                    .map(|control| box_item(control) as Box<dyn LayoutItem>)
                    .collect(),
            )?))
        })
        .expect("the canvas builds"),
    );
    let handle = canvas.surface().handle().0;
    let inside = {
        let built = built.borrow();
        [0, 1].map(|at| Stop {
            written: format!("{handle}-{}", built[at]),
            focused: inside_focus[at],
        })
    };

    let (before_focus, after_focus) = (signal(false), signal(false));
    let before_button = button(before_focus);
    let after_button = button(after_focus);
    let before = stop(&before_button, None, before_focus);
    let after = stop(&after_button, None, after_focus);
    let framed = SurfaceFrame::filling(
        Rc::clone(&canvas),
        LayoutStyle::new().width(200.0).height(100.0),
    )
    .expect("the frame builds");
    let frame = u64::from(framed.layout_node()).to_string();
    let content = Container::new(
        LayoutStyle::new().flex_column().gap(8.0),
        vec![
            box_item(before_button),
            box_item(framed),
            box_item(after_button),
        ],
    )
    .expect("the page builds");
    let root = content.layout_node();
    let tree = ComponentList::new(WindowRoot::new(box_item(content)));
    ui_core::compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(300.0),
    )
    .expect("the page lays out");

    let host: web_sys::HtmlElement = document()
        .create_element("div")
        .expect("a host element")
        .dyn_into()
        .expect("a div is an HTML element");
    document()
        .body()
        .expect("a body")
        .append_child(host.as_ref())
        .expect("the host went into the page");
    let renderer = DomRenderer::new(host.clone()).expect("a renderer on the host");
    PAGE.with(|page| {
        *page.borrow_mut() = Some(Page {
            tree,
            renderer,
            host,
            canvas,
            frame,
            before,
            inside,
            after,
        })
    });
}

/// Hands the app what the document reported since the last frame, as the runner does: focus on a window's box to the window's focus, and the rest to the overlays and the tree. Then draws the next frame.
fn render() {
    mount();
    let reported = std::mem::take(&mut *REPORTED.lock().unwrap());
    PAGE.with(|page| {
        let mut page = page.borrow_mut();
        let page = page.as_mut().expect("mounted");
        for event in &reported {
            match event {
                Event::BoxFocused { box_id } => {
                    ui_core::focus::follow_box(*box_id);
                }
                Event::FocusLeftBoxes => ui_core::focus::clear(),
                _ if ui_core::dispatch_overlays(event) == ui_core::EventResult::Ignored => {
                    page.tree.on_event(event);
                }
                _ => {}
            }
        }
        ui_core::relayout_if_dirty();
        let frame = page.tree.commands().clone();
        page.renderer
            .render_frame(&frame, None)
            .expect("the frame reconciled");
    });
}

fn with_page<R>(f: impl FnOnce(&Page) -> R) -> R {
    PAGE.with(|page| f(page.borrow().as_ref().expect("mounted")))
}

/// The document's sequential focus order inside the host, as each element is written: what the browser's Tab walks, every element of it at `tabindex="0"`.
fn tab_order() -> Vec<web_sys::HtmlElement> {
    let host = with_page(|page| page.host.clone());
    let found = host
        .query_selector_all("[tabindex]")
        .expect("a valid selector");
    (0..found.length())
        .filter_map(|index| found.item(index))
        .filter_map(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
        .filter(|element| element.tab_index() >= 0)
        .collect()
}

fn written(element: &web_sys::Element) -> String {
    element.get_attribute(ID_ATTRIBUTE).unwrap_or_default()
}

fn active() -> String {
    document()
        .active_element()
        .map(|element| written(&element))
        .unwrap_or_default()
}

async fn next_frames() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = web_sys::window()
            .expect("a window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Takes the browser's Tab one step from where focus is, forwards or backwards, and lets the app follow it.
async fn tab(backwards: bool) {
    let order = tab_order();
    let at = order
        .iter()
        .position(|element| written(element) == active());
    let next = match (at, backwards) {
        (None, false) => 0,
        (None, true) => order.len() - 1,
        (Some(at), false) => at + 1,
        (Some(at), true) => at.checked_sub(1).expect("a stop before this one"),
    };
    order[next].focus().expect("the next stop takes focus");
    next_frames().await;
    render();
    next_frames().await;
    render();
}

/// Which stop the app's focus is on, by its written name, and whether the window's focus rests on the frame while one inside it has it.
fn app_focus() -> (Vec<String>, bool) {
    with_page(|page| {
        let focused = [&page.before, &page.inside[0], &page.inside[1], &page.after]
            .into_iter()
            .filter(|stop| stop.focused.get())
            .map(|stop| stop.written.clone())
            .collect();
        let inner = {
            let _inside = page.canvas.enter();
            ui_core::focus::current().is_some()
        };
        (focused, inner && ui_core::focus::current().is_some())
    })
}

#[wasm_bindgen_test]
fn the_frame_is_no_tab_stop_of_its_own_and_its_controls_are_in_the_windows_order() {
    render();
    let (frame, stops) = with_page(|page| {
        let stops: Vec<String> = [&page.before, &page.inside[0], &page.inside[1], &page.after]
            .into_iter()
            .map(|stop| stop.written.clone())
            .collect();
        (page.frame.clone(), stops)
    });
    let order: Vec<String> = tab_order().iter().map(|element| written(element)).collect();
    assert_eq!(order, stops, "the walk runs through the frame's controls");
    assert!(!order.contains(&frame), "the frame is no stop of its own");
    for element in tab_order() {
        let keys = element.get_attribute("data-telar-keys").unwrap_or_default();
        assert!(
            !keys.split_whitespace().any(|key| key == "tab"),
            "{} keeps Tab for itself, which would stop the browser's walk: {keys}",
            written(&element)
        );
    }
}

#[wasm_bindgen_test]
async fn tab_walks_into_through_and_out_of_the_frame_and_back() {
    render();
    let (before, inside, after) = with_page(|page| {
        (
            page.before.written.clone(),
            [
                page.inside[0].written.clone(),
                page.inside[1].written.clone(),
            ],
            page.after.written.clone(),
        )
    });

    PAGE.with(|page| {
        let page = page.borrow();
        let first = tab_order()
            .into_iter()
            .find(|element| written(element) == page.as_ref().expect("mounted").before.written)
            .expect("the window's first control");
        first.focus().expect("it takes focus");
    });
    next_frames().await;
    render();
    assert_eq!(app_focus(), (vec![before.clone()], false));

    for (step, expected) in [&inside[0], &inside[1]].into_iter().enumerate() {
        tab(false).await;
        assert_eq!(active(), *expected, "step {step}: the browser's focus");
        assert_eq!(
            app_focus(),
            (vec![expected.clone()], true),
            "step {step}: the app's focus is on the control inside the frame, and the window's on the frame"
        );
    }

    tab(false).await;
    assert_eq!(active(), after);
    assert_eq!(
        app_focus(),
        (vec![after.clone()], false),
        "out of the frame, nothing inside it keeps focus"
    );

    for expected in [&inside[1], &inside[0]] {
        tab(true).await;
        assert_eq!(active(), *expected);
        assert_eq!(app_focus(), (vec![expected.clone()], true));
    }
    tab(true).await;
    assert_eq!(active(), before);
    assert_eq!(app_focus(), (vec![before], false));
}
