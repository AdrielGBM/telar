//! Shared portal-once toggleable-scrim scaffold (latch, take-once body, toggleable overlay, dismiss) behind `modal` and `drawer`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::{LayoutError, LayoutStyle};
use platform_core::Event;
use reactive_core::{Effect, RwSignal, effect};
use renderer_core::Color;
use ui_core::dismiss::{DismissId, register_dismiss, unregister_dismiss};
use ui_core::{
    Component, Container, EventResult, LayoutItem, NodeId, Overlay, ReactiveList, RenderNode,
    box_item,
};

/// Scrim tone: a translucent black wash over the page. Kept as a *fill* on an opaque-subtree dialog (never
/// an `opacity` layer over the whole overlay) so the dialog itself stays fully opaque — layering the scrim's
/// alpha over the dialog would bleed the page through it.
pub(crate) const SCRIM: Color = Color::rgba(0.0, 0.0, 0.0, 0.5);
/// Hairline border around the dialog card / drawer panel edge.
pub(crate) const DEFAULT_BORDER: Color = Color::rgba(0.12, 0.12, 0.16, 0.15);

/// The dismiss handler the helper hands to each widget's scrim (and, for `modal`, its Close): shared so both
/// affordances run the same "set `open = false`, then `on_close`" without rebuilding it per tap target.
pub(crate) type DismissFn = Rc<dyn Fn()>;

/// Carries the dismiss-stack registration effect alongside the overlay subtree, so the registration lives
/// exactly as long as the overlay is mounted and is released with it.
struct Dismissible {
    inner: Box<dyn LayoutItem>,
    _registration: Effect,
}

impl Component for Dismissible {
    fn view(&self) -> RenderNode {
        self.inner.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.inner.on_event(event)
    }
}

impl LayoutItem for Dismissible {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }
}

/// Keeps the dismiss stack in step with `open`: on the transition into open the overlay becomes the stack's
/// top, and on the way out it withdraws — including when it was closed by its own Close button rather than by
/// a dismissal, which is why the entry is tracked by id instead of assumed to still be on top.
fn track_dismissible(open: RwSignal<bool>, dismiss: DismissFn) -> Effect {
    let registered: Cell<Option<DismissId>> = Cell::new(None);
    effect(move || {
        if open.get() {
            if registered.get().is_none() {
                registered.set(Some(register_dismiss(dismiss.clone())));
            }
        } else if let Some(id) = registered.take() {
            unregister_dismiss(id);
        }
    })
}

/// A 0-size collapsed node: the unbound/never-opened placeholder that registers no overlay and blocks nothing.
fn collapsed() -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Container::new(
        LayoutStyle::new().width(0.0).height(0.0),
        vec![],
    )?))
}

/// The portal-once toggleable-scrim scaffold shared by `modal` and `drawer`. Owns the unbound guard, the
/// build latch, the take-once wrapper around `build_inner`, and the toggleable `Overlay`; `build_inner` gets
/// the shared dismiss handler and returns its own scrim container (with the card/panel inside), which is kept
/// mounted and shown/hidden via `open`.
///
/// `open` `None` (unbound) can never open, so it renders nothing and drops the captured body. Otherwise the
/// scrim is built ONCE — lazily on the first open, so the layout host exists and the portal attaches to the
/// viewport — then kept mounted and shown/hidden via `open` (see `Overlay::toggleable`), preserving the body
/// across close/reopen. The `ReactiveList` is keyed on a latch that flips true on the first open and stays
/// true, so the built scrim is never disposed.
pub(crate) fn scrim_overlay(
    open: Option<RwSignal<bool>>,
    on_close: Option<Box<dyn Fn()>>,
    build_inner: impl FnOnce(DismissFn) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Some(open) = open else {
        return collapsed();
    };

    let on_close: Option<Rc<dyn Fn()>> = on_close.map(Rc::from);
    let dismiss: DismissFn = {
        let open = open.clone();
        Rc::new(move || {
            open.set(false);
            if let Some(cb) = &on_close {
                cb();
            }
        })
    };

    // `build_inner` consumes the pre-built slot body, so it can run only once; the take-once cell lets the
    // reusable `ReactiveList` build closure invoke it on the single latch-true build and never again.
    let build_inner = RefCell::new(Some(build_inner));

    let built = Rc::new(Cell::new(false));
    let key = {
        let open = open.clone();
        let built = built.clone();
        move || {
            // Latch on first open; reading `open` subscribes the list so it re-runs to build the scrim then.
            if open.get() {
                built.set(true);
            }
            vec![built.get()]
        }
    };
    let list = ReactiveList::new(
        key,
        |b: &bool| *b,
        move |is_built| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !is_built {
                // Never opened: an empty placeholder (no overlay registered, nothing blocks the page).
                return collapsed();
            }
            let Some(build_inner) = build_inner.borrow_mut().take() else {
                return collapsed();
            };
            let inner = build_inner(dismiss.clone())?;
            // Kept mounted; shown only while `open`. So the pre-built body survives a close/reopen (not rebuilt).
            let registration = track_dismissible(open.clone(), dismiss.clone());
            let open = open.clone();
            let overlay = Overlay::toggleable(
                LayoutStyle::new().flex_column(),
                vec![box_item(inner)],
                move || open.get(),
            )?;
            Ok(box_item(Dismissible {
                inner: box_item(overlay),
                _registration: registration,
            }))
        },
    )?;
    Ok(box_item(list))
}
