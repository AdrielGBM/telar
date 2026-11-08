//! The host's side of the embedding boundary: [`load_embedded`] and the [`LoadedEmbed`] it hands back, which drives one guest instance through the entries its library exports.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use geometry_core::{Size, Transform};
use platform_core::guest::{CWaker, CallOutcome, FrameCtx, Handle, LentBytes, Timestamp};
use platform_core::{AppCtx, Event, PointerButton, SystemPreferences, WindowCommand};
use renderer_core::Color;
use renderer_core::wire::FrameDecoder;
use services_core::guest::GuestLibrary;
use ui_core::focus::FocusId;
use ui_core::{ForeignBoundary, ForeignSurface, FrameFocus, GuestSurface, KeyRoute, NodeId};
use web_time::Instant;
use wire_core::encode;

use crate::guest::GuestInstance;
use crate::{DrawList, EmbedEntries, GuestPanic, GuestStop, WindowCommands};

/// A loaded guest the host drives: one instance its library built, and the library itself, which outlives the instance. `!Send`/`!Sync`: the instance is a foreign reactive runtime, driven only on the UI thread.
///
/// Every call goes through its [`GuestLibrary`], so once the guest has panicked none reaches it again: [`panic`](Self::panic) says what happened and where, and every method answers as an empty guest would — nothing painted, no event taken, no timer waiting, nothing read. The host shows the report in the guest's place.
///
/// The guest's surface is a [`ForeignSurface`] of the host's: its frame is marked with its [`boundary`](Self::boundary), so a screen reader reads what the guest reports of it where the host shows it, and acts on it, and its keyboard sits behind the one [focus stop](Self::focus_stop) the host keeps for it, the only way keys reach it.
///
/// Its drags are the host window's. The drag the guest starts, the drop it accepts and whether the picture following the drag shows are performed for the surface the host is dispatching when the guest asks, as that surface's own [`WindowCommand`]s. A drag over the host reaches the guest only over the area it is laid out to, leaving that area reaches it as [`DragLeft`](Event::DragLeft), and the host's [`DragEnded`](Event::DragEnded) reaches it only while a drag it started is in flight.
pub struct LoadedEmbed {
    instance: Handle<GuestInstance>,
    library: Rc<GuestLibrary<EmbedEntries>>,
    surface: Rc<GuestSurface<EmbedEntries>>,
    boundary: ForeignBoundary,
    focus: RefCell<Option<Rc<FrameFocus>>>,
    size: Cell<Size>,
    /// Keeps the payloads and images consecutive frames share.
    frames: RefCell<FrameDecoder>,
    timer_mirror: TimerMirror,
    task_waker_installed: Cell<bool>,
    /// What the guest queued for the host to apply, drained early with the drag commands the host performs at once.
    held_commands: RefCell<WindowCommands>,
    drag_hovers: Cell<bool>,
    drag_started: Cell<bool>,
}

/// Loads a guest cdylib and creates one instance from it, its `create` entry handed `args`.
///
/// Each load opens a private copy of the library, so a rebuilt library at the same path never shares statics with one still mapped, a guest that panicked among them. The library is kept mapped for the guest's lifetime. An error names the library and why it could not be driven: one built against another telar, with other features, as a hot-reload library or as no telar guest at all is refused before anything of it runs but its initializers. A guest that stops while it is built, by panicking or by answering in bytes the host cannot read, comes back as the error too, a [`GuestStop`] the caller can downcast to.
///
/// # Safety
///
/// As [`open_guest`](platform_core::guest::open_guest): `path` must name an embedded guest built from a telar this host trusts. Loading runs its initializers, and once its [`EMBED_CONTRACT`](crate::EMBED_CONTRACT) matches, its [`EmbedEntries`] are called as they are, foreign code in this process.
pub unsafe fn load_embedded(
    path: &Path,
    args: &[String],
) -> Result<LoadedEmbed, Box<dyn std::error::Error>> {
    let (entries, library) = unsafe { platform_core::guest::open_guest::<EmbedEntries>(path) }
        .map_err(|why| format!("{}: {why}", path.display()))?;
    Ok(LoadedEmbed::start(entries, args, Some(library))?)
}

impl LoadedEmbed {
    /// Starts the guest `entries` were copied from, creates one instance through them and seeds it with the host's system preferences; `library` is its mapping, `None` for entries this process serves itself. A guest that stopped by then is unloaded, and why handed back.
    pub(crate) fn start(
        entries: EmbedEntries,
        args: &[String],
        library: Option<libloading::Library>,
    ) -> Result<Self, GuestStop> {
        let library = GuestLibrary::start(
            entries,
            library.map(|library| Box::new(library) as Box<dyn std::any::Any>),
            Some(renderer_text::fonts::served()),
        );
        let args = encode(&args.to_vec());
        let instance = library.answer(Handle::NULL, |entries, instance| unsafe {
            (entries.create)(LentBytes::lend(&args), instance)
        });
        let Some(instance) = instance else {
            return Err(stop_of(&library));
        };
        let library = Rc::new(library);
        let surface = Rc::new(GuestSurface::new(Rc::clone(&library), instance));
        let source: Rc<dyn ForeignSurface> = surface.clone();
        let embed = Self {
            instance,
            library,
            boundary: ForeignBoundary::new(&source),
            surface,
            focus: RefCell::default(),
            size: Cell::default(),
            frames: RefCell::default(),
            timer_mirror: TimerMirror::default(),
            task_waker_installed: Cell::new(false),
            held_commands: RefCell::default(),
            drag_hovers: Cell::new(false),
            drag_started: Cell::new(false),
        };
        // A guest starts with a store of its own that knows nothing, and a host only forwards changes; without this one opened mid-session would draw light on a dark desktop until the user next changed a setting.
        embed.set_system_preferences(&preferences_core::system_preferences());
        match embed.stopped() {
            Some(stop) => Err(stop.clone()),
            None => Ok(embed),
        }
    }

    /// Why this guest stopped, if it has: a panic, with what it said and where, or an answer the host could not read. Nothing of the guest runs after it.
    pub fn stopped(&self) -> Option<&GuestStop> {
        self.library.guest().stop()
    }

    /// What marks the guest's frame in the host's: hand it to [`composite`](crate::composite) with what [`paint`](Self::paint) drew.
    pub fn boundary(&self) -> &ForeignBoundary {
        &self.boundary
    }

    /// What the guest reports of the surface it draws, in its own coordinates and ids: what [`boundary`](Self::boundary) has whoever reads the host's frame read. It reports nothing once the guest has panicked or this is dropped.
    pub fn foreign_surface(&self) -> Rc<dyn ForeignSurface> {
        self.surface.clone()
    }

    /// Registers the one focus stop the host keeps for the guest in the box at `node` its frame is drawn in, `placement` mapping the guest's coordinates into that box's, replacing any earlier one: keys and the input method's text reach the guest only through it, and it is withdrawn once the guest panics.
    pub fn focus_stop(&self, node: NodeId, placement: impl Fn() -> Transform + 'static) -> FocusId {
        drop(self.focus.take());
        let stop = Rc::new(self.boundary.focus_stop(node, placement));
        let id = stop.id();
        *self.focus.borrow_mut() = Some(stop);
        id
    }

    /// The [`focus_stop`](Self::focus_stop)'s id, while one is registered.
    pub fn focus_stop_id(&self) -> Option<FocusId> {
        self.stop().map(|stop| stop.id())
    }

    /// Whether the key being dispatched is the guest's: its focus stop held focus when it arrived.
    pub fn holds_keys(&self) -> bool {
        self.stop().is_some_and(|stop| stop.holds_keys())
    }

    // A share of the stop rather than a borrow: what it calls can reach the guest, whose panic withdraws the stop.
    fn stop(&self) -> Option<Rc<FrameFocus>> {
        self.focus.borrow().clone()
    }

    /// Gives the stop focus when something inside the guest took it, and withdraws it once the guest has stopped.
    fn follow_guest_focus(&self) {
        if self.stopped().is_some() {
            drop(self.focus.take());
        } else if let Some(stop) = self.stop() {
            stop.follow_inside();
        }
    }

    fn send(&self, event: &Event) -> bool {
        let instance = self.instance;
        self.deliver(event, Layer::Content, |entries, event, handled| unsafe {
            (entries.on_event)(instance, event, handled)
        })
    }

    /// Hands `event` to the guest's `layer` through `entry`, then keeps up with the drag the guest heard of and performs the drag commands it queued meanwhile.
    fn deliver(
        &self,
        event: &Event,
        layer: Layer,
        entry: impl FnOnce(&EmbedEntries, LentBytes<Event>, *mut bool) -> CallOutcome,
    ) -> bool {
        let bytes = encode(event);
        let taken = self
            .library
            .ask(|entries, handled| entry(entries, LentBytes::lend(&bytes), handled));
        if taken || layer == Layer::Content {
            self.follow_drag(event);
        }
        self.take_guest_commands();
        taken
    }

    /// `event` as the guest is to hear it: a box event of the host's window under the guest's own naming, `None` for one of another surface or one whose surface the guest no longer draws; a drag outside its area as having left it, and nothing once it was told so; the end of a drag only while one it started is in flight.
    fn for_guest<'a>(&self, event: &'a Event) -> Option<Cow<'a, Event>> {
        match event {
            Event::InSurface { .. } => self.boundary.foreign_event(event).map(Cow::Owned),
            Event::DragOver { x, y, .. } | Event::Dropped { x, y, .. } if !self.covers(*x, *y) => {
                self.drag_hovers
                    .get()
                    .then_some(Cow::Owned(Event::DragLeft))
            }
            Event::DragLeft => self.drag_hovers.get().then_some(Cow::Borrowed(event)),
            Event::DragEnded { .. } => self.drag_started.get().then_some(Cow::Borrowed(event)),
            _ => Some(Cow::Borrowed(event)),
        }
    }

    fn follow_drag(&self, heard: &Event) {
        match heard {
            Event::DragOver { .. } => self.drag_hovers.set(true),
            Event::Dropped { .. } | Event::DragLeft => self.drag_hovers.set(false),
            Event::DragEnded { .. } => self.drag_started.set(false),
            _ => {}
        }
    }

    /// Drains what the guest queued: its drag commands go to the surface the host is dispatching as that surface's own, and the rest wait for [`drain_window_commands`](Self::drain_window_commands).
    fn take_guest_commands(&self) {
        let instance = self.instance;
        let drained = self
            .library
            .read(|entries, commands| unsafe {
                (entries.drain_window_commands)(instance, commands)
            })
            .unwrap_or_default();
        for command in drained {
            if !is_drag_command(&command) {
                self.held_commands.borrow_mut().push(command);
                continue;
            }
            if let WindowCommand::StartDrag(_) = command {
                self.drag_started.set(true);
            }
            platform_core::push_window_command(command);
        }
    }

    pub fn relayout(&self, width: f32, height: f32) {
        self.size.set(Size::new(width, height));
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.relayout)(instance, width, height) });
    }

    pub fn relayout_dirty(&self) {
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.relayout_dirty)(instance) });
    }

    /// The guest's frame, decoded into commands of the host's own.
    pub fn paint(&self) -> DrawList {
        let instance = self.instance;
        self.library.read_frame(
            &mut self.frames.borrow_mut(),
            |entries, frame| unsafe { (entries.paint)(instance, frame) },
            |entries, image, pixels, found| unsafe {
                (entries.pixels)(instance, image, pixels, found)
            },
            |entries| unsafe { (entries.reset_frames)(instance) },
        )
    }

    pub fn generation(&self) -> u64 {
        let instance = self.instance;
        self.library
            .answer(0, |entries, generation| unsafe {
                (entries.generation)(instance, generation)
            })
            .unwrap_or_default()
    }

    /// Hands the guest's content `event`, in the guest's own coordinates, after [`dispatch_overlays`](Self::dispatch_overlays) left it: keys go as its [focus stop](Self::focus_stop) routes them ([`KeyRoute`]), and a primary press on the guest gives the stop focus.
    pub fn on_event(&self, event: &Event) -> bool {
        let taken = match KeyRoute::of(event, self.holds_keys()) {
            Some(KeyRoute::Tab) => {
                if let Some(stop) = self.stop() {
                    stop.tab(event);
                }
                true
            }
            Some(KeyRoute::Outside) => false,
            Some(KeyRoute::Inside) => self.send(event),
            None => {
                if let Event::PointerPressed {
                    x,
                    y,
                    button: PointerButton::Primary,
                    ..
                } = event
                    && self.covers(*x, *y)
                    && let Some(stop) = self.stop()
                {
                    ui_core::focus::request_from_pointer(stop.id());
                }
                self.for_guest(event).is_some_and(|event| self.send(&event))
            }
        };
        self.follow_guest_focus();
        taken
    }

    /// Offers `event`, in the guest's own coordinates, to the guest's overlays first; `true` means one took it and its content is not to hear it. Keys reach them as [`on_event`](Self::on_event) routes them, Tab excepted, which is left to it.
    pub fn dispatch_overlays(&self, event: &Event) -> bool {
        let reaches = match KeyRoute::of(event, self.holds_keys()) {
            Some(KeyRoute::Inside) => Some(Cow::Borrowed(event)),
            Some(KeyRoute::Tab | KeyRoute::Outside) => None,
            None => self.for_guest(event),
        };
        let instance = self.instance;
        let taken = reaches.is_some_and(|event| {
            self.deliver(&event, Layer::Overlays, |entries, event, handled| unsafe {
                (entries.dispatch_overlays)(instance, event, handled)
            })
        });
        self.follow_guest_focus();
        taken
    }

    /// Whether `(x, y)`, in the guest's coordinates, falls in the area it is laid out to: a press the host forwards for an overlay to dismiss on lands outside it, and is no reason to take the host's focus, and a drag outside it is not over the guest.
    fn covers(&self, x: f64, y: f64) -> bool {
        let size = self.size.get();
        (0.0..size.width as f64).contains(&x) && (0.0..size.height as f64).contains(&y)
    }

    /// Call once per frame the host drove this guest through, after its events. Closes the guest's one-frame keyboard state, which `key_pressed` inside it answers from.
    pub fn end_frame(&self) {
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.end_frame)(instance) });
    }

    pub fn motion_tick(&self, now: Instant) {
        let instance = self.instance;
        let now = self.library.epoch().stamp(now);
        self.library
            .call(|entries| unsafe { (entries.motion_tick)(instance, now) });
    }

    pub fn motion_has_active(&self) -> bool {
        let instance = self.instance;
        self.library
            .ask(|entries, active| unsafe { (entries.motion_has_active)(instance, active) })
    }

    /// The window commands the guest queued that are the host's to apply or ignore, oldest first: every one but its drags, which the host's surface already performed.
    pub fn drain_window_commands(&self) -> WindowCommands {
        self.take_guest_commands();
        self.held_commands.take()
    }

    /// Hands the guest a new snapshot of the user's system preferences. [`load_embedded`] seeds it with the host's own; call this from the host's `on_system_preferences` so the guest follows later changes.
    pub fn set_system_preferences(&self, preferences: &SystemPreferences) {
        let instance = self.instance;
        self.library
            .send(preferences, |entries, preferences| unsafe {
                (entries.set_system_preferences)(instance, preferences)
            });
    }

    /// Tells the guest its content became visible, which may focus a field of its own: the focus stop then takes focus, so the keys reach it.
    pub fn activate(&self) {
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.activate)(instance) });
        self.follow_guest_focus();
    }

    pub fn clear_color(&self) -> Option<Color> {
        let instance = self.instance;
        self.library
            .read(|entries, color| unsafe { (entries.clear_color)(instance, color) })
            .flatten()
    }

    pub fn title(&self) -> String {
        let instance = self.instance;
        self.library
            .read(|entries, title| unsafe { (entries.title)(instance, title) })
            .unwrap_or_default()
    }

    pub fn icon(&self) -> Option<Vec<u8>> {
        let instance = self.instance;
        self.library
            .read(|entries, icon| unsafe { (entries.icon)(instance, icon) })
            .flatten()
    }

    pub fn id(&self) -> String {
        let instance = self.instance;
        self.library
            .read(|entries, id| unsafe { (entries.id)(instance, id) })
            .unwrap_or_default()
    }

    /// Call once per frame. Hands the guest the host's loop waker for its tasks the first time there is one, runs the guest's due timers, its finished tasks and its own hook, then keeps the host's loop waking for the guest's next timer.
    pub fn on_frame(&self, ctx: &mut AppCtx) {
        if !self.task_waker_installed.get()
            && let Some(waker) = ctx.redraw_waker()
        {
            let waker = CWaker::new(waker.clone());
            self.library
                .call(move |entries| unsafe { (entries.install_task_waker)(waker) });
            self.task_waker_installed.set(true);
        }
        let instance = self.instance;
        FrameCtx::lend(ctx, |frame| {
            self.library
                .call(|entries| unsafe { (entries.on_frame)(instance, frame) })
        });
        self.timer_mirror
            .follow(self.next_timer_due(), self.until_next_timer());
        self.follow_guest_focus();
    }

    /// How long until the guest's earliest timer comes due; `None` when none is waiting. [`on_frame`](Self::on_frame) already keeps the host's loop waking for it.
    pub fn until_next_timer(&self) -> Option<Duration> {
        let instance = self.instance;
        self.library
            .read(|entries, after| unsafe { (entries.until_next_timer)(instance, after) })
            .flatten()
    }

    pub(crate) fn next_timer_due(&self) -> Option<Instant> {
        let instance = self.instance;
        let mut found = false;
        let due = self
            .library
            .answer(Timestamp::default(), |entries, due| unsafe {
                (entries.next_timer_due)(instance, due, &mut found)
            })?;
        found.then(|| self.library.epoch().instant(due))
    }
}

/// Which of the guest's layers an event is handed to: its overlays have first refusal, and its content hears what they leave.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Overlays,
    Content,
}

/// Whether `command` starts or answers a drag, which the window has to hear while it dispatches the event that asked.
fn is_drag_command(command: &WindowCommand) -> bool {
    match command {
        WindowCommand::StartDrag(_)
        | WindowCommand::AcceptDrop(_)
        | WindowCommand::ShowDragIcon(_) => true,
        WindowCommand::Drag
        | WindowCommand::DragResize(_)
        | WindowCommand::SetDecorations(_)
        | WindowCommand::Minimize
        | WindowCommand::ToggleMaximize
        | WindowCommand::SetMaximized(_)
        | WindowCommand::Close
        | WindowCommand::SetTitle(_)
        | WindowCommand::Focus
        | WindowCommand::SetCursor(_)
        | WindowCommand::Navigate(_) => false,
    }
}

fn stop_of(library: &GuestLibrary<EmbedEntries>) -> GuestStop {
    library.guest().stop().cloned().unwrap_or_else(|| {
        GuestStop::Panicked(GuestPanic {
            message: "the guest built no instance".to_owned(),
            location: None,
        })
    })
}

/// The guest's earliest timer, held as a timer of the host's own. The host's loop sleeps until the earliest deadline in the host's runtime, and the guest's timers are in a copy of their own it cannot see: this one runs nothing, it only wakes the frame in which the host drives the guest's [`on_frame`](LoadedEmbed::on_frame).
#[derive(Default)]
pub(crate) struct TimerMirror(RefCell<Option<(Instant, reactive_core::Timer)>>);

impl TimerMirror {
    /// Follows the guest's earliest deadline, `due` on its clock and `wait` from now. A deadline already followed is left alone: scheduling it again would wake the loop every frame for a timer that has not moved.
    pub(crate) fn follow(&self, due: Option<Instant>, wait: Option<Duration>) {
        let mut mirrored = self.0.borrow_mut();
        let following = mirrored
            .as_ref()
            .is_some_and(|(at, timer)| Some(*at) == due && timer.is_pending());
        if !following {
            *mirrored = due
                .zip(wait)
                .map(|(due, wait)| (due, reactive_core::run_after(wait, || {})));
        }
    }
}

impl Drop for LoadedEmbed {
    // A guest that panicked keeps its instance: releasing it would run that guest's code again. Its library is unloaded after, as the field drops, unless a share of its surface someone kept holds it until that goes.
    fn drop(&mut self) {
        drop(self.focus.take());
        self.surface.forget();
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.release)(instance) });
    }
}

#[cfg(test)]
#[path = "host.test.rs"]
mod tests;
