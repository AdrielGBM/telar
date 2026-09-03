//! The animation-frame loop, and the listeners that feed it.

use std::cell::RefCell;

use platform_core::{Event, EventHandler, Platform, PlatformError, Window, WindowConfig};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::dom;
use crate::map;
use crate::window::WebWindow;

// The queue is separate from the running app on purpose: a listener must be able to record an event while a
// frame is running, and the browser can dispatch one synchronously from inside our own code — focusing an
// element, for one. Sharing a cell with the running handler would make that a panic.
thread_local! {
    static QUEUE: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static FRAME: RefCell<Frame> = const { RefCell::new(Frame { scheduled: false, callback: None }) };
}

struct Frame {
    scheduled: bool,
    callback: Option<Closure<dyn FnMut()>>,
}

struct App {
    handler: Box<dyn EventHandler<WebWindow>>,
    window: WebWindow,
    /// Kept alive for the life of the app: dropping a `Closure` unregisters the listener behind it.
    _listeners: Vec<Listener>,
}

/// One registered DOM listener, removed when dropped.
struct Listener {
    target: web_sys::EventTarget,
    event: &'static str,
    closure: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .target
            .remove_event_listener_with_callback(self.event, self.closure.as_ref().unchecked_ref());
    }
}

/// Asks for a frame. Idempotent within one turn of the browser's loop, so a burst of events costs one frame.
///
/// This is what [`WebWindow::request_redraw`] and the process-global loop waker both reach, and the reason
/// neither needs to hold anything: the scheduler is thread-local, and on this target there is one thread.
pub fn request_frame() {
    FRAME.with(|frame| {
        let mut frame = frame.borrow_mut();
        if frame.scheduled {
            return;
        }
        let Some(callback) = frame.callback.as_ref() else {
            return;
        };
        if dom::window()
            .request_animation_frame(callback.as_ref().unchecked_ref())
            .is_ok()
        {
            frame.scheduled = true;
        }
    });
}

fn push(events: impl IntoIterator<Item = Event>) {
    QUEUE.with(|queue| queue.borrow_mut().extend(events));
    request_frame();
}

#[derive(Clone, Debug)]
pub struct WebPlatformConfig {
    /// A CSS selector for the element the app fills. `None` mounts on `<body>`.
    pub host: Option<String>,
    /// Whether to give the host element keyboard focus once it is mounted. On by default: an app that
    /// occupies the page expects to be typed into without being clicked first.
    pub autofocus: bool,
    /// Whether to set the host's `touch-action` and `overscroll-behavior` so a drag inside the app does not
    /// scroll or bounce the page. On by default; an app embedded in a scrolling document turns it off.
    pub owns_gestures: bool,
}

impl Default for WebPlatformConfig {
    fn default() -> Self {
        Self {
            host: None,
            autofocus: true,
            owns_gestures: true,
        }
    }
}

pub struct WebPlatform {
    config: WebPlatformConfig,
    /// The element to mount on, when the caller already resolved it. Whoever builds the renderer has to put
    /// its canvas somewhere, so it resolves the host first and hands the same one over rather than letting
    /// the two look it up independently and disagree.
    host: Option<web_sys::HtmlElement>,
}

impl WebPlatform {
    pub fn new(config: WebPlatformConfig) -> Self {
        Self { config, host: None }
    }

    pub fn with_host(host: web_sys::HtmlElement, config: WebPlatformConfig) -> Self {
        Self {
            config,
            host: Some(host),
        }
    }
}

impl Default for WebPlatform {
    fn default() -> Self {
        Self::new(WebPlatformConfig::default())
    }
}

impl Platform for WebPlatform {
    type Window = WebWindow;

    /// Mounts the app and returns, leaving it running on the browser's loop.
    ///
    /// Every other platform blocks here until the app closes. A browser has one thread and owns the loop, so
    /// blocking would freeze the page: the app is handed to `requestAnimationFrame` and the listeners, and
    /// this returns to whatever called `main`.
    fn run<H: EventHandler<WebWindow> + 'static>(
        self,
        config: WindowConfig,
        mut handler: H,
    ) -> Result<(), PlatformError> {
        let host = match self.host.clone() {
            Some(host) => host,
            None => dom::host(self.config.host.as_deref()).map_err(PlatformError)?,
        };
        prepare_host(&host, &self.config);
        dom::document().set_title(&config.title);

        let window = WebWindow::new(host.clone());
        let listeners = install_listeners(&host, &window);

        // A capture-free closure, so it satisfies the `Send + Sync` the waker is declared with. It reaches a
        // thread-local, which on a target with one thread is the same thing as reaching the loop.
        platform_core::set_loop_waker(std::sync::Arc::new(request_frame));

        install_frame_callback();

        handler.new_events();
        let resumed = handler.on_resume(&window);
        handler.about_to_wait();
        if !resumed {
            return Err(PlatformError(
                "the browser handler refused to resume (the renderer could not be built)".into(),
            ));
        }

        APP.with(|app| {
            *app.borrow_mut() = Some(App {
                handler: Box::new(handler),
                window,
                _listeners: listeners,
            })
        });
        request_frame();
        Ok(())
    }
}

fn prepare_host(host: &web_sys::HtmlElement, config: &WebPlatformConfig) {
    let style = host.style();
    if config.owns_gestures {
        // Without these a drag inside the app pans the page under it, and a fling at the edge triggers the
        // browser's own overscroll — both of which read as the app losing the gesture.
        let _ = style.set_property("touch-action", "none");
        let _ = style.set_property("overscroll-behavior", "contain");
    }
    // Focusable but not in the tab order: the app manages focus within itself, and a page that embeds it
    // should not gain a stop that lands on an opaque box.
    if host.get_attribute("tabindex").is_none() {
        let _ = host.set_attribute("tabindex", "-1");
    }
    let _ = style.set_property("outline", "none");
    if config.autofocus {
        let _ = host.focus();
    }
}

fn listen(
    target: &web_sys::EventTarget,
    event: &'static str,
    passive: bool,
    handler: impl FnMut(web_sys::Event) + 'static,
) -> Listener {
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(handler);
    let options = web_sys::AddEventListenerOptions::new();
    options.set_passive(passive);
    let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
        event,
        closure.as_ref().unchecked_ref(),
        &options,
    );
    Listener {
        target: target.clone(),
        event,
        closure,
    }
}

fn install_listeners(host: &web_sys::HtmlElement, window: &WebWindow) -> Vec<Listener> {
    let target: &web_sys::EventTarget = host.as_ref();
    let mut listeners = Vec::new();

    for (name, kind) in [
        ("pointerdown", PointerKind::Down),
        ("pointermove", PointerKind::Move),
        ("pointerup", PointerKind::Up),
        ("pointercancel", PointerKind::Cancel),
    ] {
        let window = window.clone();
        let host = host.clone();
        listeners.push(listen(target, name, false, move |event| {
            let Ok(event) = event.dyn_into::<web_sys::PointerEvent>() else {
                return;
            };
            on_pointer(&window, &host, kind, &event);
        }));
    }

    {
        let window = window.clone();
        listeners.push(listen(target, "wheel", false, move |event| {
            let Ok(event) = event.dyn_into::<web_sys::WheelEvent>() else {
                return;
            };
            // The app scrolls its own panes, so the page must not scroll underneath them.
            event.prevent_default();
            let (x, y) = window.to_local(event.client_x() as f64, event.client_y() as f64);
            push([Event::Scrolled {
                delta: map::scroll_delta(&event),
                x,
                y,
            }]);
        }));
    }

    for (name, pressed) in [("keydown", true), ("keyup", false)] {
        listeners.push(listen(target, name, false, move |event| {
            let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>() else {
                return;
            };
            on_key(pressed, &event);
        }));
    }

    for (name, focused) in [("focusin", true), ("focusout", false)] {
        let host = host.clone();
        listeners.push(listen(target, name, true, move |event| {
            // Both bubble, so the host is told about every focus move *inside* it — and a backend whose
            // output is a document fills it with real buttons and fields for focus to move between. Only
            // focus crossing the host's own border is the window gaining or losing it: reporting the rest
            // told the app the window had gone away, and every widget dutifully ended the gesture the click
            // that moved the focus had just begun.
            if let Some(event) = event.dyn_ref::<web_sys::FocusEvent>()
                && let Some(other) = event.related_target()
                && let Some(node) = other.dyn_ref::<web_sys::Node>()
                && host.contains(Some(node))
            {
                return;
            }
            push([Event::FocusChanged {
                is_focused: focused,
            }]);
        }));
    }

    listeners.push(listen(target, "pointerenter", true, move |_| {
        push([Event::CursorEntered]);
    }));
    listeners.push(listen(target, "pointerleave", true, move |_| {
        push([Event::CursorLeft]);
    }));

    // A right-click belongs to the app, which draws its own menus.
    listeners.push(listen(target, "contextmenu", false, move |event| {
        event.prevent_default();
    }));

    let viewport: web_sys::EventTarget = dom::window().into();
    listeners.push(listen(&viewport, "resize", true, move |_| {
        // The turn re-measures and synthesises the events; this only has to wake it.
        request_frame();
    }));

    if let Ok(Some(query)) = dom::window().match_media("(prefers-color-scheme: dark)") {
        let target: web_sys::EventTarget = query.into();
        listeners.push(listen(&target, "change", true, move |event| {
            let Ok(event) = event.dyn_into::<web_sys::MediaQueryListEvent>() else {
                return;
            };
            push([Event::ColorSchemeChanged {
                dark: event.matches(),
            }]);
        }));
    }

    listeners
}

#[derive(Clone, Copy)]
enum PointerKind {
    Down,
    Move,
    Up,
    Cancel,
}

fn on_pointer(
    window: &WebWindow,
    host: &web_sys::HtmlElement,
    kind: PointerKind,
    event: &web_sys::PointerEvent,
) {
    let (x, y) = window.to_local(event.client_x() as f64, event.client_y() as f64);
    let source = map::source_of(event);
    let modifiers = map::mouse_modifiers(event);
    let mut events = vec![Event::ModifiersChanged { modifiers }];
    match kind {
        PointerKind::Move => events.push(Event::PointerMoved { x, y, source }),
        PointerKind::Down => {
            let Some(button) = map::button_of(event.button()) else {
                return;
            };
            // Capture, so a drag that leaves the element keeps arriving — which is what a slider or a
            // resize handle needs, and what the browser otherwise stops at the boundary.
            let _ = host.set_pointer_capture(event.pointer_id());
            let _ = host.focus();
            events.extend(map::pointer_pressed(x, y, button, source));
        }
        PointerKind::Up => {
            let Some(button) = map::button_of(event.button()) else {
                return;
            };
            let _ = host.release_pointer_capture(event.pointer_id());
            events.push(Event::PointerReleased {
                x,
                y,
                button,
                source,
            });
        }
        // A cancelled pointer never comes back up, so the press is released where it was last seen rather
        // than left held — a finger the browser took for a system gesture must not stick a button down.
        PointerKind::Cancel => {
            let _ = host.release_pointer_capture(event.pointer_id());
            events.push(Event::PointerReleased {
                x,
                y,
                button: platform_core::PointerButton::Primary,
                source,
            });
            events.push(Event::CursorLeft);
        }
    }
    push(events);
}

fn on_key(pressed: bool, event: &web_sys::KeyboardEvent) {
    let modifiers = map::key_modifiers(event);
    let Some(key) = map::key_of(&event.key()) else {
        return;
    };
    // A chord the browser owns — copy, paste, reload, a new tab — stays the browser's.
    if !modifiers.is_ctrl && !modifiers.is_meta && map::key_steals_default(&key) {
        event.prevent_default();
    }
    let event = if pressed {
        Event::KeyPressed { key, modifiers }
    } else {
        Event::KeyReleased { key, modifiers }
    };
    push([Event::ModifiersChanged { modifiers }, event]);
}

fn install_frame_callback() {
    let callback = Closure::<dyn FnMut()>::new(move || {
        FRAME.with(|frame| frame.borrow_mut().scheduled = false);
        turn();
    });
    FRAME.with(|frame| frame.borrow_mut().callback = Some(callback));
}

/// One pass of the loop: drain what arrived, draw, and decide whether another is due.
///
/// The app is taken out of its cell for the duration, so a listener the browser dispatches synchronously
/// from inside a frame finds nothing to re-enter and simply queues its event for the next one.
fn turn() {
    let Some(mut app) = APP.with(|app| app.borrow_mut().take()) else {
        return;
    };

    app.handler.new_events();

    let measured = app.window.measure();
    if measured.scale {
        app.handler.on_event(
            Event::ScaleFactorChanged {
                scale_factor: app.window.scale_factor(),
            },
            &app.window,
        );
    }
    if measured.size {
        // The logical size, not the device one: what this drives is layout, which works in CSS pixels.
        let (width, height) = app.window.logical_size();
        app.handler
            .on_event(Event::WindowResized { width, height }, &app.window);
    }

    let events = QUEUE.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    for event in events {
        app.handler.on_event(event, &app.window);
    }

    app.handler.on_redraw(&app.window);
    let wants_another = app.handler.about_to_wait().is_some();

    if app.handler.take_exit_request() {
        app.handler.on_suspend();
        return;
    }

    APP.with(|slot| *slot.borrow_mut() = Some(app));
    if wants_another {
        request_frame();
    }
}
