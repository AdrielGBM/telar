//! [`GuestLibrary`]: one guest a host loaded, driven through its [`Entries`] the same way whichever boundary declared them.

use std::any::Any;
use std::sync::Arc;

use platform_core::guest::{
    CallOutcome, Entries, Epoch, Guest, HostServices, LentBytes, host_epoch,
};
use renderer_core::wire::{FrameDecoder, FrameError};
use renderer_core::{DrawCommand, FontHost};
use wire_core::{Wire, encode};

use super::ServedServices;

/// One guest a host loaded: the entries copied out of its library, the [`Guest`] every call goes through, the epoch the timestamps it is sent count from, and what it was loaded with, kept until it is unloaded.
///
/// Dropping it unloads the guest through its [`Lifecycle`](platform_core::guest::Lifecycle), once whatever holds its instances has released them: `unloading` unless it panicked, then `quiesce`, then its library is unmapped and the backends its services call into are freed. A `quiesce` that panics leaves a thread that may still run the guest's code and call those services, so the library and the services are kept together, for the rest of the process.
pub struct GuestLibrary<E: Entries> {
    guest: Guest,
    entries: E,
    epoch: Epoch,
    loaded: Option<Loaded>,
}

/// What a library loaded from a file is kept with: its mapping, and the backends its services table points into.
struct Loaded {
    _library: Box<dyn Any>,
    services: ServedServices,
}

impl<E: Entries> GuestLibrary<E> {
    /// Starts the guest `entries` were copied from, handing it the host's epoch and services before anything else of it runs. A panic in any of them is recorded in its [`Guest`] rather than returned.
    ///
    /// `library` is what unmaps the guest when dropped, as [`open_guest`](platform_core::guest::open_guest) hands it back: the guest is told it will be unloaded, adopts an epoch read now and is served this side's stores, with `fonts` the database its shaper defers to, as [`ServedServices::installed`] takes it. `None` for entries this process serves itself, which share its epoch and its stores already: they count from [`host_epoch`], are served nothing and are never unloaded.
    pub fn start(
        entries: E,
        library: Option<Box<dyn Any>>,
        fonts: Option<Arc<dyn FontHost>>,
    ) -> Self {
        let loaded = library.map(|library| Loaded {
            _library: library,
            services: ServedServices::installed(fonts),
        });
        let table = loaded
            .as_ref()
            .map_or(HostServices::NONE, |loaded| loaded.services.table());
        let started = Self {
            guest: Guest::default(),
            entries,
            epoch: if loaded.is_some() {
                Epoch::start()
            } else {
                host_epoch()
            },
            loaded,
        };
        if started.loaded.is_some() {
            started.call(|entries| unsafe { (entries.lifecycle().loaded)() });
            let now = started.epoch.now();
            started.call(|entries| unsafe { (entries.lifecycle().adopt_epoch)(now) });
        }
        started.call(|entries| unsafe { (entries.lifecycle().install_services)(table) });
        started
    }

    /// Whether the guest has panicked, and what the panic said.
    pub fn guest(&self) -> &Guest {
        &self.guest
    }

    /// The host's reading of the moment every timestamp the guest is sent or answers with counts from.
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// Calls one entry, unless the guest has already panicked: `None` when it had, or does now.
    pub fn call(&self, entry: impl FnOnce(&E) -> CallOutcome) -> Option<()> {
        self.guest
            .call(|| unsafe { entry(&self.entries).into_result() })
    }

    /// Calls an entry that answers through a pointer to a `T`, which starts out as `initial`.
    pub fn answer<T>(
        &self,
        initial: T,
        entry: impl FnOnce(&E, *mut T) -> CallOutcome,
    ) -> Option<T> {
        let mut out = initial;
        self.call(|entries| entry(entries, &mut out))?;
        Some(out)
    }

    /// Calls an entry that answers yes or no, as no once the guest has panicked.
    pub fn ask(&self, entry: impl FnOnce(&E, *mut bool) -> CallOutcome) -> bool {
        self.answer(false, entry).unwrap_or(false)
    }

    /// Calls an entry that answers with bytes it lends, and reads them into a `T` of this side's own before anything else of the guest runs.
    pub fn read<T: Wire>(
        &self,
        entry: impl FnOnce(&E, *mut LentBytes) -> CallOutcome,
    ) -> Option<T> {
        let lent = self.answer(LentBytes::EMPTY, entry)?;
        unsafe { lent.decode() }
            .inspect_err(
                |error| tracing::warn!(%error, "the guest answered with bytes this host cannot read"),
            )
            .ok()
    }

    /// Lends `value`, spelled as bytes, to one entry for the call.
    pub fn send(
        &self,
        value: &impl Wire,
        entry: impl FnOnce(&E, LentBytes) -> CallOutcome,
    ) -> Option<()> {
        let bytes = encode(value);
        self.call(|entries| entry(entries, LentBytes::lend(&bytes)))
    }

    /// The frame an instance lends through `paint`, decoded by `frames` into commands of this side's own, the pixels of an image it has not met copied through `pixels`; no commands once the guest has panicked.
    ///
    /// A frame that cannot be read leaves the guest's encoder a step ahead of the decoder, so `reset` has the guest make the next one stand alone, and that one is read in its place.
    pub fn read_frame(
        &self,
        frames: &mut FrameDecoder,
        paint: impl Fn(&E, *mut LentBytes) -> CallOutcome,
        pixels: impl Fn(&E, u64, *mut LentBytes, *mut bool) -> CallOutcome,
        reset: impl FnOnce(&E) -> CallOutcome,
    ) -> Vec<DrawCommand> {
        let mut commands = Vec::new();
        if let Some(Err(error)) = self.decode_frame(frames, &mut commands, &paint, &pixels) {
            tracing::warn!(%error, "a frame the guest drew could not be read, so it draws the next one on its own");
            self.call(reset);
            if let Some(Err(error)) = self.decode_frame(frames, &mut commands, &paint, &pixels) {
                tracing::error!(%error, "a frame the guest drew on its own could not be read either");
            }
        }
        commands
    }

    fn decode_frame(
        &self,
        frames: &mut FrameDecoder,
        commands: &mut Vec<DrawCommand>,
        paint: &impl Fn(&E, *mut LentBytes) -> CallOutcome,
        pixels: &impl Fn(&E, u64, *mut LentBytes, *mut bool) -> CallOutcome,
    ) -> Option<Result<(), FrameError>> {
        let frame = self.answer(LentBytes::EMPTY, paint)?;
        // The pixels entry is the one call that leaves the frame it reads from lent.
        let frame = unsafe { frame.as_slice() };
        Some(frames.decode(frame, commands, |image| {
            let mut found = false;
            let lent = self.answer(LentBytes::EMPTY, |entries, lent| {
                pixels(entries, image, lent, &mut found)
            })?;
            found.then(|| unsafe { lent.as_slice() }.to_vec())
        }))
    }
}

impl<E: Entries> Drop for GuestLibrary<E> {
    fn drop(&mut self) {
        let Some(loaded) = self.loaded.take() else {
            return;
        };
        let lifecycle = *self.entries.lifecycle();
        self.guest.unload(
            loaded,
            || unsafe { (lifecycle.unloading)().into_result() },
            || unsafe { (lifecycle.quiesce)().into_result() },
        );
    }
}

#[cfg(test)]
#[path = "library_test.rs"]
mod tests;
