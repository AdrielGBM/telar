//! The browser's session history as the app's [`LocationSource`]: `pushState`/`replaceState` out, `popstate` and `hashchange` in, and the document's scroll position kept per entry.

use std::cell::RefCell;
use std::rc::Rc;

use js_sys::{Array, Object, Reflect};
use platform_core::{Event, Location, LocationFormat, LocationSource};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::Closure;

use crate::dom;
use crate::platform::{Listener, listen};

const STATE_KEY: &str = "telar";

/// The browser's session history, as the app's address.
///
/// Every entry carries the app's whole history in its `history.state`, so a reload or a traversal restores the stack the entry was made from rather than a single page, and the document's scroll position, so back and forward return to where the reader was. The browser's own scroll restoration is turned off, since it would restore before the app has drawn the page it belongs to.
///
/// `?telar-*` page settings are kept out of the locations the app sees and written back onto every address it pushes, so a setting made by a link survives navigating.
pub struct WebLocation {
    shared: Rc<Shared>,
    _listeners: Vec<Listener>,
}

struct Shared {
    format: LocationFormat,
    page_settings: Vec<(String, String)>,
    history: RefCell<Vec<Location>>,
}

impl WebLocation {
    /// Addresses written in `format`; `None` takes the base from the page's `<base href>`, and `/` where it has none.
    pub fn new(format: Option<LocationFormat>) -> Self {
        let format = format.unwrap_or_else(base_from_document);
        platform_core::set_location_format(format.clone());
        let shared = Rc::new(Shared {
            format,
            page_settings: page_settings(),
            history: RefCell::new(Vec::new()),
        });
        let viewport: web_sys::EventTarget = dom::window().into();
        let listeners = vec![
            {
                let shared = shared.clone();
                listen(&viewport, "popstate", true, move |event| {
                    let state = event
                        .dyn_ref::<web_sys::PopStateEvent>()
                        .map(|event| event.state())
                        .unwrap_or(JsValue::NULL);
                    shared.traversed(state);
                })
            },
            {
                let shared = shared.clone();
                listen(&viewport, "hashchange", true, move |_| {
                    shared.traversed(browser_history().state().unwrap_or(JsValue::NULL))
                })
            },
            {
                let shared = shared.clone();
                listen(&viewport, "pagehide", true, move |_| shared.stamp_current())
            },
        ];
        Self {
            shared,
            _listeners: listeners,
        }
    }

    pub fn format(&self) -> &LocationFormat {
        &self.shared.format
    }
}

impl LocationSource for WebLocation {
    fn initial(&mut self) -> Vec<Location> {
        let _ = browser_history().set_scroll_restoration(web_sys::ScrollRestoration::Manual);
        let shared = &self.shared;
        let current = shared.current();
        let stored = read_state(
            &browser_history().state().unwrap_or(JsValue::NULL),
            &shared.format,
        );
        let history = match (current, stored) {
            (Some(current), Some((history, scroll))) if history.last() == Some(&current) => {
                restore_scroll(scroll);
                history
            }
            (current, _) => current.into_iter().collect(),
        };
        *shared.history.borrow_mut() = history.clone();
        shared.stamp_current();
        history
    }

    fn push(&mut self, history: &[Location]) {
        let shared = &self.shared;
        shared.stamp_current();
        let Some(top) = history.last() else {
            return;
        };
        let state = write_state(history, (0.0, 0.0), &shared.format);
        let _ = browser_history().push_state_with_url(&state, "", Some(&shared.href(top)));
        *shared.history.borrow_mut() = history.to_vec();
        if top.fragment().is_none() {
            dom::window().scroll_to_with_x_and_y(0.0, 0.0);
        }
    }

    fn replace(&mut self, history: &[Location]) {
        let shared = &self.shared;
        let Some(top) = history.last() else {
            return;
        };
        let state = write_state(history, scroll_position(), &shared.format);
        let _ = browser_history().replace_state_with_url(&state, "", Some(&shared.href(top)));
        *shared.history.borrow_mut() = history.to_vec();
    }

    // The entries the app stepped back over are kept, so the browser's forward button can return to them.
    fn back(&mut self, count: usize, history: &[Location]) {
        self.shared.stamp_current();
        *self.shared.history.borrow_mut() = history.to_vec();
        let _ = browser_history().go_with_delta(-(count.min(i32::MAX as usize) as i32));
    }
}

impl Shared {
    fn current(&self) -> Option<Location> {
        let location = dom::window().location();
        let reference = format!(
            "{}{}{}",
            location.pathname().ok()?,
            location.search().unwrap_or_default(),
            location.hash().unwrap_or_default()
        );
        let parsed = self.format.parse(&reference)?;
        Some(without_page_settings(parsed))
    }

    fn href(&self, location: &Location) -> String {
        let with_settings = self
            .page_settings
            .iter()
            .fold(location.clone(), |location, (key, value)| {
                location.with_param(key.clone(), value.clone())
            });
        self.format.format(&with_settings)
    }

    /// Writes the app's history and the current scroll position into the entry the browser is showing, so leaving it — by a push, a traversal or a reload — can come back to both.
    fn stamp_current(&self) {
        let state = write_state(&self.history.borrow(), scroll_position(), &self.format);
        let _ = browser_history().replace_state(&state, "");
    }

    /// The browser moved to another entry by itself. An entry this app stamped says which history it belongs to; any other is a new one the browser opened on top — a same-page link, an address edited by hand — and is stamped now.
    fn traversed(&self, state: JsValue) {
        let Some(current) = self.current() else {
            return;
        };
        let known = self.history.borrow().clone();
        let history = match read_state(&state, &self.format) {
            Some((history, scroll)) if history.last() == Some(&current) => {
                restore_scroll(scroll);
                history
            }
            _ if known.last() == Some(&current) => return,
            _ => {
                let mut history = known.clone();
                history.push(current);
                *self.history.borrow_mut() = history.clone();
                self.stamp_current();
                history
            }
        };
        if history == known {
            return;
        }
        *self.history.borrow_mut() = history.clone();
        platform_core::post_event(Event::LocationChanged { history });
    }
}

fn browser_history() -> web_sys::History {
    dom::window()
        .history()
        .expect("a browser window has a history")
}

fn scroll_position() -> (f64, f64) {
    let window = dom::window();
    (
        window.scroll_x().unwrap_or_default(),
        window.scroll_y().unwrap_or_default(),
    )
}

type FrameLoop = Rc<RefCell<Option<Closure<dyn FnMut()>>>>;

/// Scrolls the document back to where an entry was left, once the app has drawn enough of the page to reach it: a page adopted from history is laid out over the next frames, and a position past its current end would be clamped short.
fn restore_scroll((x, y): (f64, f64)) {
    const FRAMES: u32 = 30;
    let attempt: FrameLoop = Rc::new(RefCell::new(None));
    let again = attempt.clone();
    let mut left = FRAMES;
    *attempt.borrow_mut() = Some(Closure::new(move || {
        let window = dom::window();
        window.scroll_to_with_x_and_y(x, y);
        let (at_x, at_y) = scroll_position();
        left -= 1;
        if ((at_x - x).abs() < 1.0 && (at_y - y).abs() < 1.0) || left == 0 {
            again.borrow_mut().take();
            return;
        }
        if let Some(callback) = again.borrow().as_ref() {
            let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
        }
    }));
    if let Some(callback) = attempt.borrow().as_ref() {
        let _ = dom::window().request_animation_frame(callback.as_ref().unchecked_ref());
    }
}

fn write_state(history: &[Location], (x, y): (f64, f64), format: &LocationFormat) -> JsValue {
    let entries: Array = history
        .iter()
        .map(|location| JsValue::from_str(&format.format(location)))
        .collect();
    let telar = Object::new();
    let _ = Reflect::set(&telar, &"history".into(), &entries);
    let _ = Reflect::set(&telar, &"scrollX".into(), &x.into());
    let _ = Reflect::set(&telar, &"scrollY".into(), &y.into());
    let state = Object::new();
    let _ = Reflect::set(&state, &STATE_KEY.into(), &telar);
    state.into()
}

fn read_state(state: &JsValue, format: &LocationFormat) -> Option<(Vec<Location>, (f64, f64))> {
    let telar = Reflect::get(state, &STATE_KEY.into()).ok()?;
    let entries: Array = Reflect::get(&telar, &"history".into())
        .ok()?
        .dyn_into()
        .ok()?;
    let history: Vec<Location> = entries
        .iter()
        .filter_map(|entry| format.parse(&entry.as_string()?))
        .collect();
    if history.is_empty() {
        return None;
    }
    let number = |key: &str| {
        Reflect::get(&telar, &key.into())
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or_default()
    };
    Some((history, (number("scrollX"), number("scrollY"))))
}

fn is_page_setting(key: &str) -> bool {
    key.starts_with("telar-")
}

fn without_page_settings(location: Location) -> Location {
    if !location
        .params()
        .iter()
        .any(|(key, _)| is_page_setting(key))
    {
        return location;
    }
    let mut kept = Location::from_segments(location.segments().iter().cloned());
    for (key, value) in location
        .params()
        .iter()
        .filter(|(key, _)| !is_page_setting(key))
    {
        kept = kept.with_param(key.clone(), value.clone());
    }
    match location.fragment() {
        Some(fragment) => kept.with_fragment(fragment),
        None => kept,
    }
}

fn page_settings() -> Vec<(String, String)> {
    let search = dom::window().location().search().unwrap_or_default();
    LocationFormat::root()
        .parse(&search)
        .map(|location| {
            location
                .params()
                .iter()
                .filter(|(key, _)| is_page_setting(key))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn base_from_document() -> LocationFormat {
    let document = dom::document();
    let declared = document
        .query_selector("base[href]")
        .ok()
        .flatten()
        .is_some();
    let base = declared
        .then(|| document.base_uri().ok().flatten())
        .flatten()
        .and_then(|uri| web_sys::Url::new(&uri).ok())
        .map(|url| url.pathname())
        .map(|path| path[..=path.rfind('/').unwrap_or(0)].to_string());
    base.map(|path| LocationFormat::new(&path))
        .unwrap_or_default()
}
