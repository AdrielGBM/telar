//! Wayland's `wl_data_device`, driven on winit's own connection: winit exposes neither the device nor the button serial a drag must start with, so this binds the seat's data device and a second pointer on the same seat to read that serial from.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::sync::{Arc, Mutex, PoisonError};

use platform_core::{
    DragActions, DragData, DragIcon, DragOffer, DragOutcome, DropAction, DropAnswer, DroppedData,
    Event, MimeType, OutgoingDrag,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_backend::sys::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_data_source::{self, WlDataSource};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::protocol::wl_shm::{self, WlShm};
use wayland_client::protocol::wl_shm_pool::WlShmPool;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

/// Never dropped: its proxies live on winit's display, and destroying them after winit has closed it at exit crashes inside libwayland. The compositor reclaims them when the socket closes.
pub(super) struct Device {
    connection: Connection,
    queue: Mutex<(EventQueue<State>, State)>,
    handle: QueueHandle<State>,
    received: Arc<Mutex<Vec<(WindowId, Event)>>>,
}

struct State {
    manager: WlDataDeviceManager,
    device: WlDataDevice,
    _pointer: WlPointer,
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    held_press: Option<u32>,
    surfaces: HashMap<ObjectId, (WindowId, WlSurface)>,
    offers: HashMap<ObjectId, Offered>,
    hovered: Option<Hovered>,
    outgoing: Option<Outgoing>,
    events: Vec<(WindowId, Event)>,
    received: Arc<Mutex<Vec<(WindowId, Event)>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

struct Offered {
    offer: WlDataOffer,
    types: Vec<MimeType>,
    actions: Option<DndAction>,
}

struct Hovered {
    offer: WlDataOffer,
    window: WindowId,
    serial: u32,
    at: (f64, f64),
    answer: Option<DropAnswer>,
}

struct Outgoing {
    source: WlDataSource,
    data: DragData,
    origin: WindowId,
    icon: Option<IconSurface>,
    icon_shown: bool,
    action: Option<DropAction>,
    dropped: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Device {
    pub(super) fn connect(
        event_loop: &ActiveEventLoop,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<&'static Self> {
        let RawDisplayHandle::Wayland(display) = event_loop.display_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: winit keeps this `wl_display` alive for the whole loop, and a foreign backend never closes it.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<State>(&connection).ok()?;
        let handle = queue.handle();
        let seat: WlSeat = globals.bind(&handle, 1..=9, ()).ok()?;
        let manager: WlDataDeviceManager = globals.bind(&handle, 1..=3, ()).ok()?;
        let device = manager.get_data_device(&seat, &handle, ());
        let pointer = seat.get_pointer(&handle, ());
        let received = Arc::new(Mutex::new(Vec::new()));
        let state = State {
            manager,
            device,
            _pointer: pointer,
            compositor: globals.bind(&handle, 1..=6, ()).ok(),
            shm: globals.bind(&handle, 1..=1, ()).ok(),
            held_press: None,
            surfaces: HashMap::new(),
            offers: HashMap::new(),
            hovered: None,
            outgoing: None,
            events: Vec::new(),
            received: received.clone(),
            wake,
        };
        let _ = connection.flush();
        Some(Box::leak(Box::new(Self {
            connection,
            queue: Mutex::new((queue, state)),
            handle,
            received,
        })))
    }

    pub(super) fn attach(&self, window: &Window) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Wayland(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: the pointer is winit's live `wl_surface` for this window, on the display this connection shares.
        let Ok(id) =
            (unsafe { ObjectId::from_ptr(WlSurface::interface(), handle.surface.as_ptr().cast()) })
        else {
            return;
        };
        if let Ok(surface) = WlSurface::from_id(&self.connection, id.clone()) {
            lock(&self.queue)
                .1
                .surfaces
                .insert(id, (window.id(), surface));
        }
    }

    pub(super) fn detach(&self, window: WindowId) {
        lock(&self.queue)
            .1
            .surfaces
            .retain(|_, (owner, _)| *owner != window);
    }

    pub(super) fn dispatch(&self) -> Vec<(WindowId, Event)> {
        let mut events = {
            let mut guard = lock(&self.queue);
            let (queue, state) = &mut *guard;
            let _ = queue.dispatch_pending(state);
            std::mem::take(&mut state.events)
        };
        events.append(&mut lock(&self.received));
        let _ = self.connection.flush();
        events
    }

    pub(super) fn start_drag(&self, window: WindowId, drag: OutgoingDrag) -> bool {
        let mut guard = lock(&self.queue);
        let (queue, state) = &mut *guard;
        let _ = queue.dispatch_pending(state);
        let Some(serial) = state.held_press else {
            return false;
        };
        let Some(origin) = state.surface_of(window) else {
            return false;
        };
        if state.outgoing.is_some() || drag.data.is_empty() {
            return false;
        }
        let source = state.manager.create_data_source(&self.handle, ());
        for mime in drag.data.types() {
            source.offer(mime.to_string());
        }
        if source.version() >= 3 {
            source.set_actions(actions_of(drag.actions));
        }
        let icon = match (&drag.icon, &state.compositor, &state.shm) {
            (Some(icon), Some(compositor), Some(shm)) => {
                IconSurface::new(compositor, shm, &self.handle, icon)
            }
            _ => None,
        };
        state.device.start_drag(
            Some(&source),
            &origin,
            icon.as_ref().map(|icon| &icon.surface),
            serial,
        );
        if let Some(icon) = &icon {
            icon.show();
        }
        state.outgoing = Some(Outgoing {
            source,
            data: drag.data,
            origin: window,
            icon,
            icon_shown: true,
            action: None,
            dropped: false,
        });
        drop(guard);
        let _ = self.connection.flush();
        true
    }

    pub(super) fn answer_drag(&self, window: WindowId, answer: Option<DropAnswer>) {
        {
            let mut guard = lock(&self.queue);
            let Some(hovered) = guard.1.hovered.as_mut().filter(|h| h.window == window) else {
                return;
            };
            if hovered.answer == answer {
                return;
            }
            let offer = &hovered.offer;
            offer.accept(
                hovered.serial,
                answer.as_ref().map(|answer| answer.mime.to_string()),
            );
            if offer.version() >= 3 {
                let action = answer
                    .as_ref()
                    .map_or(DndAction::empty(), |answer| action_of(answer.action));
                offer.set_actions(action, action);
            }
            hovered.answer = answer;
        }
        let _ = self.connection.flush();
    }

    pub(super) fn show_drag_icon(&self, shown: bool) {
        {
            let mut guard = lock(&self.queue);
            let Some(outgoing) = guard.1.outgoing.as_mut() else {
                return;
            };
            if std::mem::replace(&mut outgoing.icon_shown, shown) == shown {
                return;
            }
            let Some(icon) = &outgoing.icon else {
                return;
            };
            if shown {
                icon.show();
            } else {
                icon.hide();
            }
        }
        let _ = self.connection.flush();
    }
}

fn action_of(action: DropAction) -> DndAction {
    match action {
        DropAction::Copy => DndAction::Copy,
        DropAction::Move => DndAction::Move,
    }
}

fn actions_of(actions: DragActions) -> DndAction {
    match actions {
        DragActions::Copy => DndAction::Copy,
        DragActions::Move => DndAction::Move,
        DragActions::CopyOrMove => DndAction::Copy | DndAction::Move,
    }
}

/// `None` for a source that offers neither copying nor moving, which nothing here can perform.
fn drag_actions(offered: DndAction) -> Option<DragActions> {
    match (
        offered.contains(DndAction::Copy),
        offered.contains(DndAction::Move),
    ) {
        (true, true) => Some(DragActions::CopyOrMove),
        (true, false) => Some(DragActions::Copy),
        (false, true) => Some(DragActions::Move),
        (false, false) => None,
    }
}

impl State {
    fn surface_of(&self, window: WindowId) -> Option<WlSurface> {
        self.surfaces
            .values()
            .find(|(owner, _)| *owner == window)
            .map(|(_, surface)| surface.clone())
    }

    fn offer_of(&self, offer: &WlDataOffer) -> Option<DragOffer> {
        let offered = self.offers.get(&offer.id())?;
        // A source on a version-1 or -2 device announces no actions, and copying is what those always meant.
        let actions = match offered.actions {
            Some(actions) => drag_actions(actions)?,
            None => DragActions::Copy,
        };
        Some(DragOffer {
            types: offered.types.clone(),
            actions,
        })
    }

    fn hover(&mut self) {
        let Some(hovered) = &self.hovered else {
            return;
        };
        let Some(offer) = self.offer_of(&hovered.offer) else {
            return;
        };
        let (x, y) = hovered.at;
        self.events
            .push((hovered.window, Event::DragOver { x, y, offer }));
    }

    fn drop_here(&mut self) {
        let Some(hovered) = self.hovered.take() else {
            return;
        };
        let offer = hovered.offer;
        let (x, y) = hovered.at;
        let Some(answer) = hovered.answer else {
            self.events.push((hovered.window, Event::DragLeft));
            return;
        };
        let Ok((mut reader, writer)) = std::io::pipe() else {
            self.events.push((hovered.window, Event::DragLeft));
            return;
        };
        offer.receive(answer.mime.to_string(), writer.as_fd());
        drop(writer);
        if offer.version() >= 3 {
            offer.finish();
        }
        offer.destroy();
        self.offers.remove(&offer.id());
        let received = self.received.clone();
        let wake = self.wake.clone();
        let window = hovered.window;
        // The source may be this very process, answering on the queue this thread dispatches, so reading here would wait on itself.
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if reader.read_to_end(&mut bytes).is_err() {
                return;
            }
            let data = DroppedData {
                mime: answer.mime,
                bytes: bytes.into(),
                action: answer.action,
            };
            lock(&received).push((window, Event::Dropped { x, y, data }));
            wake();
        });
    }

    fn end_outgoing(&mut self, outcome: DragOutcome) {
        let Some(outgoing) = self.outgoing.take() else {
            return;
        };
        outgoing.source.destroy();
        self.events
            .push((outgoing.origin, Event::DragEnded { outcome }));
    }
}

/// The picture the compositor moves under the pointer: a surface of the drag-icon role over one `wl_shm` buffer.
struct IconSurface {
    surface: WlSurface,
    buffer: WlBuffer,
    pool: WlShmPool,
    scale: i32,
    hotspot: (i32, i32),
    size: (i32, i32),
    _file: File,
}

impl IconSurface {
    fn new(
        compositor: &WlCompositor,
        shm: &WlShm,
        handle: &QueueHandle<State>,
        icon: &DragIcon,
    ) -> Option<Self> {
        let (width, height) = icon.size();
        let (width, height) = (i32::try_from(width).ok()?, i32::try_from(height).ok()?);
        let stride = width.checked_mul(4)?;
        let length = stride.checked_mul(height)?;
        let fd = rustix::fs::memfd_create(
            c"telar-drag-icon",
            rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING,
        )
        .ok()?;
        let mut file = File::from(fd);
        // `Argb8888` is a little-endian word per pixel, so its bytes run blue, green, red, alpha; both sides are premultiplied.
        let pixels: Vec<u8> = icon
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, a]| [b, g, r, a])
            .collect();
        file.write_all(&pixels).ok()?;
        let _ = rustix::fs::fcntl_add_seals(
            &file,
            rustix::fs::SealFlags::SHRINK | rustix::fs::SealFlags::SEAL,
        );
        let pool = shm.create_pool(file.as_fd(), length, handle, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            stride,
            wl_shm::Format::Argb8888,
            handle,
            (),
        );
        let surface = compositor.create_surface(handle, ());
        Some(Self {
            surface,
            buffer,
            pool,
            scale: i32::try_from(icon.scale()).unwrap_or(1),
            hotspot: icon.hotspot(),
            size: (width, height),
            _file: file,
        })
    }

    fn show(&self) {
        let surface = &self.surface;
        let (x, y) = self.hotspot;
        if surface.version() >= 3 {
            surface.set_buffer_scale(self.scale);
        }
        // From version 5 a non-zero offset on `attach` is a protocol error, and `offset` carries it instead.
        if surface.version() >= 5 {
            surface.attach(Some(&self.buffer), 0, 0);
            surface.offset(-x, -y);
        } else {
            surface.attach(Some(&self.buffer), -x, -y);
        }
        if surface.version() >= 4 {
            surface.damage_buffer(0, 0, self.size.0, self.size.1);
        } else {
            surface.damage(0, 0, i32::MAX, i32::MAX);
        }
        surface.commit();
    }

    fn hide(&self) {
        self.surface.attach(None, 0, 0);
        self.surface.commit();
    }
}

impl Drop for IconSurface {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
        self.surface.destroy();
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

macro_rules! ignores_events {
    ($($ty:ty),+) => {
        $(impl Dispatch<$ty, ()> for State {
            fn event(
                _: &mut Self,
                _: &$ty,
                _: <$ty as Proxy>::Event,
                _: &(),
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
            }
        })+
    };
}

ignores_events!(
    WlSeat,
    WlDataDeviceManager,
    WlCompositor,
    WlShm,
    WlShmPool,
    WlBuffer,
    WlSurface
);

impl Dispatch<WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let wl_pointer::Event::Button {
            serial,
            state: WEnum::Value(button),
            ..
        } = event
        else {
            return;
        };
        match button {
            wl_pointer::ButtonState::Pressed => state.held_press = Some(serial),
            wl_pointer::ButtonState::Released => {
                state.held_press = None;
                // A drag the compositor took holds the pointer, so a release still reaching it means the drag never began: the press was over before `start_drag` arrived.
                if state.outgoing.as_ref().is_some_and(|o| !o.dropped) {
                    state.end_outgoing(DragOutcome::NotStarted);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataSource, ()> for State {
    fn event(
        state: &mut Self,
        source: &WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(outgoing) = state
            .outgoing
            .as_mut()
            .filter(|outgoing| outgoing.source == *source)
        else {
            return;
        };
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let Ok(mime) = MimeType::parse(mime_type) else {
                    return;
                };
                let Some(bytes) = outgoing.data.bytes(&mime).cloned() else {
                    return;
                };
                // A reader in another process may take its time, and this thread is the UI's.
                std::thread::spawn(move || {
                    let _ = File::from(fd).write_all(&bytes);
                });
            }
            wl_data_source::Event::Action { dnd_action } => {
                outgoing.action = match dnd_action {
                    WEnum::Value(action) if action.contains(DndAction::Move) => {
                        Some(DropAction::Move)
                    }
                    WEnum::Value(action) if action.contains(DndAction::Copy) => {
                        Some(DropAction::Copy)
                    }
                    _ => None,
                };
            }
            wl_data_source::Event::DndDropPerformed => outgoing.dropped = true,
            wl_data_source::Event::DndFinished => {
                let action = outgoing.action.unwrap_or(DropAction::Copy);
                state.end_outgoing(DragOutcome::Dropped(action));
            }
            wl_data_source::Event::Cancelled => state.end_outgoing(DragOutcome::Cancelled),
            _ => {}
        }
    }
}

impl Dispatch<WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(offered) = state.offers.get_mut(&offer.id()) else {
            return;
        };
        match event {
            wl_data_offer::Event::Offer { mime_type } => {
                if let Ok(mime) = MimeType::parse(mime_type)
                    && !offered.types.contains(&mime)
                {
                    offered.types.push(mime);
                }
            }
            wl_data_offer::Event::SourceActions {
                source_actions: WEnum::Value(actions),
            } => {
                offered.actions = Some(actions);
                if state.hovered.as_ref().is_some_and(|h| h.offer == *offer) {
                    state.hover();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                let current = state.hovered.as_ref().map(|h| h.offer.id());
                state.offers.retain(|known, offered| {
                    let kept = Some(known) == current.as_ref();
                    if !kept {
                        offered.offer.destroy();
                    }
                    kept
                });
                state.offers.insert(
                    id.id(),
                    Offered {
                        offer: id,
                        types: Vec::new(),
                        actions: None,
                    },
                );
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                if let Some(left) = state.hovered.take() {
                    state.events.push((left.window, Event::DragLeft));
                }
                let window = state.surfaces.get(&surface.id()).map(|(window, _)| *window);
                let (Some(window), Some(offer)) = (window, id) else {
                    return;
                };
                state.hovered = Some(Hovered {
                    offer,
                    window,
                    serial,
                    at: (x, y),
                    answer: None,
                });
                if let Some(hovered) = &state.hovered {
                    hovered.offer.accept(serial, None);
                }
                state.hover();
            }
            wl_data_device::Event::Motion { x, y, .. } => {
                if let Some(hovered) = state.hovered.as_mut() {
                    hovered.at = (x, y);
                    state.hover();
                }
            }
            wl_data_device::Event::Leave => {
                if let Some(left) = state.hovered.take() {
                    state.events.push((left.window, Event::DragLeft));
                }
            }
            wl_data_device::Event::Drop => state.drop_here(),
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}
