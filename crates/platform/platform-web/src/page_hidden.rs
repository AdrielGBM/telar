//! Hearing that the page is going out of view: reloaded, closed, navigated away from, or sent to the background, where a browser may discard it without running another line.

use std::rc::Rc;

use crate::dom;
use crate::platform::{Listener, listen};

/// Calls a function each time the page is hidden, for as long as it is held: on `pagehide`, which a reload, a close or a navigation fires, and on a `visibilitychange` that leaves the document hidden, the last moment a tab sent to the background is sure to run. What has to outlive the page is written then.
pub struct PageHidden {
    _listeners: [Listener; 2],
}

impl PageHidden {
    pub fn new(on_hidden: impl Fn() + 'static) -> Self {
        let on_hidden = Rc::new(on_hidden);
        let leaving = on_hidden.clone();
        let window: web_sys::EventTarget = dom::window().into();
        let document: web_sys::EventTarget = dom::document().into();
        Self {
            _listeners: [
                listen(&window, "pagehide", true, move |_| leaving()),
                listen(&document, "visibilitychange", true, move |_| {
                    if dom::document().hidden() {
                        on_hidden();
                    }
                }),
            ],
        }
    }
}
