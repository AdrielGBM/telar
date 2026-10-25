//! Following a [`Destination`]: a route is pushed onto the app's history, an anchor is added to the history and revealed where it is, and an external URI is handed to the system.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use layout_core::NodeId;
use platform_core::{Destination, ModifiersState, RouteAddress};
use reactive_core::{SurfaceHandle, current_surface};
use rustc_hash::FxHashMap;

use crate::surface_context::HomeSurface;

type Reveal = Rc<dyn Fn() -> bool>;
type ReadDestination = Rc<dyn Fn() -> Option<Destination>>;
pub(crate) type ReadRun = Rc<dyn Fn(usize) -> Option<Destination>>;

reactive_core::dropless_thread_local! {
    static ANCHORS: released FxHashMap<Arc<str>, Reveal> = FxHashMap::default();
    // Keyed by surface too: every surface mints node ids from its own counter, so a node id alone names a different box in each.
    static LINKS: released FxHashMap<(SurfaceHandle, NodeId), (u64, ReadDestination)> = FxHashMap::default();
    static RUNS: released FxHashMap<(SurfaceHandle, NodeId), (u64, ReadRun)> = FxHashMap::default();
    static PENDING: released Option<Arc<str>> = const { None };
    static REVEALER_INSTALLED: Cell<bool> = const { Cell::new(false) };
    static NEXT_TOKEN: Cell<u64> = const { Cell::new(1) };
}

/// Goes where `destination` points, in place: pushes the route, reveals the anchor, switches the locale or opens the URI. `false` when nothing went anywhere — a route with no page, an anchor no box answers to, a locale the address does not carry, a URI nothing opened.
pub fn follow(destination: &Destination) -> bool {
    match destination {
        Destination::Route(location) => platform_core::push_location(location.clone()),
        Destination::Anchor(name) => {
            if !has_anchor(name) {
                return false;
            }
            platform_core::push_anchor(name);
            true
        }
        Destination::Locale(locale) => {
            let carried = platform_core::location_locales()
                .iter()
                .any(|carried| carried.eq_ignore_ascii_case(locale));
            if !carried {
                return false;
            }
            let shown = platform_core::location_history().pop().unwrap_or_default();
            platform_core::push_location(shown.with_locale(locale.as_ref()))
        }
        Destination::External(uri) => services_core::open_uri(uri),
    }
}

/// Goes where `destination` points in a view beside this one where the target has one — a new browser tab for a route — and in place where it does not.
pub fn follow_beside(destination: &Destination) -> bool {
    if matches!(destination, Destination::Route(_)) {
        match RouteAddress::parse(&platform_core::address_of(destination)) {
            Ok(address) => {
                if services_core::open_beside(&address) {
                    return true;
                }
            }
            Err(error) => tracing::warn!("a route's address is not opened beside: {error}"),
        }
    }
    follow(destination)
}

/// Follows `destination` the way a press with `modifiers` held asks to: Ctrl, Cmd or Shift ask for a view beside this one, as they do on a web page.
pub fn follow_pressed(destination: &Destination, modifiers: ModifiersState) -> bool {
    if modifiers.is_ctrl || modifiers.is_meta || modifiers.is_shift {
        follow_beside(destination)
    } else {
        follow(destination)
    }
}

/// Makes `name` an anchor a link can reveal, until the returned registration drops. A later registration of the same name replaces an earlier one, and says so: two places answering to one name is a mistake a link cannot show.
///
/// What `anchor:` registers for its box; `reveal` scrolls the box into place and reports whether it could.
#[must_use = "the anchor is withdrawn when the registration drops"]
pub fn register_anchor(name: &str, reveal: impl Fn() -> bool + 'static) -> AnchorRegistration {
    install_revealer();
    let name: Arc<str> = name.into();
    let reveal: Reveal = Rc::new(reveal);
    let replaced =
        ANCHORS.with(|anchors| anchors.borrow_mut().insert(name.clone(), reveal.clone()));
    if replaced.is_some() {
        tracing::warn!("two boxes are anchors named \"{name}\"; a link to it reaches the last one");
    }
    AnchorRegistration { name, reveal }
}

/// Reveals the anchor named `name`, reporting whether one was registered under it.
pub fn reveal_anchor(name: &str) -> bool {
    let reveal = ANCHORS.with(|anchors| anchors.borrow().get(name).cloned());
    reveal.is_some_and(|reveal| reveal())
}

/// Whether a box answers to the anchor `name` right now.
pub fn has_anchor(name: &str) -> bool {
    ANCHORS.with(|anchors| anchors.borrow().contains_key(name))
}

fn install_revealer() {
    if REVEALER_INSTALLED.replace(true) {
        return;
    }
    platform_core::set_anchor_revealer(reveal_when_ready);
}

/// Reveals `name` now, and again whenever its box moves, until the reader moves the page themselves.
///
/// An address that names an anchor arrives before the page is laid out, and the place it names moves while the page finishes: a face arriving remeasures the text above it, an image takes its size. Revealing once would leave the reader wherever the anchor was at that moment.
fn reveal_when_ready(name: &str) {
    PENDING.with(|pending| *pending.borrow_mut() = Some(name.into()));
    reveal_anchor(name);
}

/// The anchor `name`'s box moved: revealed again if it is the one still being arrived at.
pub(crate) fn anchor_moved(name: &str) {
    let arriving = PENDING.with(|pending| pending.borrow().as_deref() == Some(name));
    if arriving {
        reveal_anchor(name);
    }
}

/// The anchor `from` took the name `to`: an arrival still pulling the page toward it keeps doing so, and the history names it by its new name.
pub(crate) fn anchor_renamed(from: &str, to: &str) {
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        if pending.as_deref() == Some(from) {
            *pending = Some(to.into());
        }
    });
    platform_core::rename_anchor(from, to);
}

/// The reader pressed, scrolled or typed: wherever the page is now is theirs, and an anchor still being arrived at stops pulling it back.
pub fn reader_moved() {
    PENDING.with(|pending| pending.borrow_mut().take());
}

/// Keeps an anchor registered; see [`register_anchor`].
pub struct AnchorRegistration {
    name: Arc<str>,
    reveal: Reveal,
}

impl Drop for AnchorRegistration {
    fn drop(&mut self) {
        ANCHORS.with(|anchors| {
            let mut anchors = anchors.borrow_mut();
            if anchors
                .get(&self.name)
                .is_some_and(|current| Rc::ptr_eq(current, &self.reveal))
            {
                anchors.remove(&self.name);
            }
        });
    }
}

/// Follows the link on the box `box_id` names, for a surface that activated it by itself (see [`Event::BoxActivated`](platform_core::Event::BoxActivated)). `false` when the box is gone, is no link, or has nowhere to go right now.
pub fn activate_box(box_id: u64) -> bool {
    let key = (current_surface(), NodeId::from(box_id));
    let read = LINKS.with(|links| links.borrow().get(&key).map(|(_, read)| read.clone()));
    read.and_then(|read| read())
        .is_some_and(|destination| follow(&destination))
}

/// Follows the link on span `run` of the text whose box is `box_id`, for a surface that activated it by itself (see [`Event::RunActivated`](platform_core::Event::RunActivated)). `false` when the text is gone or that span links nowhere.
pub fn activate_run(box_id: u64, run: u32) -> bool {
    let key = (current_surface(), NodeId::from(box_id));
    let read = RUNS.with(|runs| runs.borrow().get(&key).map(|(_, read)| read.clone()));
    read.and_then(|read| read(run as usize))
        .is_some_and(|destination| follow(&destination))
}

fn next_token() -> u64 {
    NEXT_TOKEN.with(|next| next.replace(next.get() + 1))
}

/// Forgets every link and run of the active surface: its node ids restart with the layout runtime, so what is left under them would answer for the next tree's boxes.
pub(crate) fn reset() {
    let surface = current_surface();
    LINKS.with(|links| links.borrow_mut().retain(|(owner, _), _| *owner != surface));
    RUNS.with(|runs| runs.borrow_mut().retain(|(owner, _), _| *owner != surface));
}

/// Registrations carry a token, so a widget outliving a reset cannot withdraw what a newer widget registered under a recycled node id.
pub(crate) fn register_runs(node: NodeId, read: ReadRun) -> u64 {
    let token = next_token();
    RUNS.with(|runs| {
        runs.borrow_mut()
            .insert((current_surface(), node), (token, read))
    });
    token
}

pub(crate) fn unregister_runs(home: HomeSurface, node: NodeId, token: u64) {
    RUNS.with(|runs| {
        let mut runs = runs.borrow_mut();
        let key = (home.handle(), node);
        if runs.get(&key).is_some_and(|(held, _)| *held == token) {
            runs.remove(&key);
        }
    });
}

pub(crate) fn register_link(node: NodeId, read: ReadDestination) -> u64 {
    let token = next_token();
    LINKS.with(|links| {
        links
            .borrow_mut()
            .insert((current_surface(), node), (token, read))
    });
    token
}

pub(crate) fn unregister_link(home: HomeSurface, node: NodeId, token: u64) {
    LINKS.with(|links| {
        let mut links = links.borrow_mut();
        let key = (home.handle(), node);
        if links.get(&key).is_some_and(|(held, _)| *held == token) {
            links.remove(&key);
        }
    });
}

/// Whether the surface follows links by itself. A document does: its `<a>` answers the click, Enter and a reader, opens a new tab on a modified click, and reports a plain one back as [`Event::BoxActivated`](platform_core::Event::BoxActivated). Following it here as well would go twice.
pub(crate) fn surface_follows_links() -> bool {
    ui_tree::element_capture()
}

#[cfg(test)]
#[path = "link_test.rs"]
mod tests;
