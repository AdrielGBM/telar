//! A Wayland `wl_shm` `Argb8888` present path for the software renderer, used when the app asks for a transparent surface.
//!
//! softbuffer only offers opaque `Xrgb8888`, so this bypasses it and manages its own alpha-preserving shm buffers — mirroring softbuffer's own Wayland backend (from which the shm/pool/release plumbing is adapted) and the Android `ANativeWindow` bypass, so all three present paths honor transparency consistently. The connection is built from the *foreign* display pointer, so it shares the app's existing Wayland display; present runs on the same thread as the surface's event loop, exactly like the softbuffer path it replaces.

use std::fs::File;
use std::io;
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use memmap2::MmapMut;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

use super::pixels::{PixelFormat, convert_rgba, convert_rgba_region, copy_region};
use super::present::{FrameOp, PresentLog, SurfaceDamage, declared_damage, note_damage};

/// The event-dispatch sink. Only `wl_buffer.release` carries state (flips the buffer's `released` flag); the rest are inert because we drive everything with explicit requests.
struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut State,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(
        _: &mut State,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(
        _: &mut State,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, Arc<AtomicBool>> for State {
    fn event(
        _: &mut State,
        _: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        released: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let wl_buffer::Event::Release = event {
            released.store(true, Ordering::SeqCst);
        }
    }
}

fn create_memfile() -> io::Result<File> {
    use rustix::fs::{MemfdFlags, SealFlags};
    let fd = rustix::fs::memfd_create(
        c"telar-alpha-shm",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )?;
    // Sealing lets the compositor mmap the fd read-only without worrying it might shrink underneath.
    let _ = rustix::fs::fcntl_add_seals(&fd, SealFlags::SHRINK | SealFlags::SEAL);
    Ok(File::from(fd))
}

fn frame_bytes(width: i32, height: i32) -> io::Result<i32> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "frame exceeds a wl_shm pool's i32 size",
            )
        })
}

#[derive(Debug, PartialEq)]
enum PoolResize {
    Keep,
    Grow(i32),
    Recreate(i32),
}

// `wl_shm_pool.resize` can only grow a pool, and the memfd is sealed against shrinking, so memory is reclaimed by a new pool — worth it only once a frame needs less than half of the old one.
fn pool_resize(pool_size: i32, needed: i32) -> PoolResize {
    if needed > pool_size {
        PoolResize::Grow(needed)
    } else if needed < pool_size / 2 {
        PoolResize::Recreate(needed)
    } else {
        PoolResize::Keep
    }
}

struct ShmFile {
    file: File,
    map: MmapMut,
}

impl ShmFile {
    fn new(size: i32) -> io::Result<Self> {
        let file = create_memfile()?;
        file.set_len(size as u64)?;
        // SAFETY: the memfd is shared only with the compositor, which never truncates it; the shrink seal forbids it.
        let map = unsafe { MmapMut::map_mut(&file)? };
        Ok(Self { file, map })
    }

    fn size(&self) -> i32 {
        self.map.len() as i32
    }

    fn fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }

    fn fit(&mut self, needed: i32) -> io::Result<PoolResize> {
        let change = pool_resize(self.size(), needed);
        match change {
            PoolResize::Keep => {}
            PoolResize::Grow(size) => {
                self.file.set_len(size as u64)?;
                // SAFETY: as in `new`.
                self.map = unsafe { MmapMut::map_mut(&self.file)? };
            }
            PoolResize::Recreate(size) => *self = ShmFile::new(size)?,
        }
        Ok(change)
    }

    fn pixels(&self, count: usize) -> &[u32] {
        // SAFETY: any byte pattern is a valid u32.
        let (prefix, pixels, _) = unsafe { self.map[..count * 4].align_to::<u32>() };
        assert!(prefix.is_empty(), "an mmap starts on a page boundary");
        pixels
    }

    fn pixels_mut(&mut self, count: usize) -> &mut [u32] {
        // SAFETY: any byte pattern is a valid u32.
        let (prefix, pixels, _) = unsafe { self.map[..count * 4].align_to_mut::<u32>() };
        assert!(prefix.is_empty(), "an mmap starts on a page boundary");
        pixels
    }
}

fn create_buffer(
    pool: &wl_shm_pool::WlShmPool,
    width: i32,
    height: i32,
    qh: &QueueHandle<State>,
    released: &Arc<AtomicBool>,
) -> wl_buffer::WlBuffer {
    pool.create_buffer(
        0,
        width,
        height,
        width * 4,
        wl_shm::Format::Argb8888,
        qh,
        released.clone(),
    )
}

struct Buf {
    file: ShmFile,
    pool: wl_shm_pool::WlShmPool,
    buffer: wl_buffer::WlBuffer,
    width: i32,
    height: i32,
    released: Arc<AtomicBool>,
}

impl Buf {
    fn new(
        shm: &wl_shm::WlShm,
        width: i32,
        height: i32,
        qh: &QueueHandle<State>,
    ) -> io::Result<Self> {
        let size = frame_bytes(width, height)?;
        let file = ShmFile::new(size)?;
        let pool = shm.create_pool(file.fd(), size, qh, ());
        let released = Arc::new(AtomicBool::new(true));
        let buffer = create_buffer(&pool, width, height, qh, &released);
        Ok(Self {
            file,
            pool,
            buffer,
            width,
            height,
            released,
        })
    }

    fn resize(
        &mut self,
        shm: &wl_shm::WlShm,
        width: i32,
        height: i32,
        qh: &QueueHandle<State>,
    ) -> io::Result<()> {
        if self.width == width && self.height == height {
            return Ok(());
        }
        match self.file.fit(frame_bytes(width, height)?)? {
            PoolResize::Keep => {}
            PoolResize::Grow(size) => self.pool.resize(size),
            PoolResize::Recreate(size) => {
                let pool = shm.create_pool(self.file.fd(), size, qh, ());
                std::mem::replace(&mut self.pool, pool).destroy();
            }
        }
        let buffer = create_buffer(&self.pool, width, height, qh, &self.released);
        std::mem::replace(&mut self.buffer, buffer).destroy();
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn attach(&self, surface: &wl_surface::WlSurface) {
        self.released.store(false, Ordering::SeqCst);
        surface.attach(Some(&self.buffer), 0, 0);
    }

    fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

impl AsRef<[u32]> for Buf {
    fn as_ref(&self) -> &[u32] {
        self.file.pixels(self.pixel_count())
    }
}

impl AsMut<[u32]> for Buf {
    fn as_mut(&mut self) -> &mut [u32] {
        let count = self.pixel_count();
        self.file.pixels_mut(count)
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
    }
}

struct Slot<B> {
    buffer: B,
    // Presents since the buffer was last filled: 1 for the one on screen, 0 when its contents are unknown.
    age: u8,
    size: (usize, usize),
}

// Two buffers presented in turn, so the compositor can hold one while the other is filled.
struct Swapchain<B> {
    front: Slot<B>,
    back: Slot<B>,
}

impl<B: AsRef<[u32]> + AsMut<[u32]>> Swapchain<B> {
    fn new(front: B, back: B) -> Self {
        let slot = |buffer| Slot {
            buffer,
            age: 0,
            size: (0, 0),
        };
        Self {
            front: slot(front),
            back: slot(back),
        }
    }

    fn front(&self) -> &B {
        &self.front.buffer
    }

    fn back(&self) -> &B {
        &self.back.buffer
    }

    fn back_mut(&mut self) -> &mut B {
        &mut self.back.buffer
    }

    // The back catches up on earlier presents by copying from the front, so only this present's changes are converted.
    fn swap_in(&mut self, log: &PresentLog, rgba: &[u8], width: usize, height: usize) -> FrameOp {
        let size = (width, height);
        let back_age = if self.back.size == size {
            self.back.age
        } else {
            0
        };
        let plan = log.plan(back_age);
        let front_current = self.front.age != 0 && self.front.size == size;
        let back = self.back.buffer.as_mut();
        let damage = if !front_current || matches!(plan.changed, FrameOp::Full) {
            convert_rgba(rgba, back, PixelFormat::Argb8888);
            FrameOp::Full
        } else {
            let front = self.front.buffer.as_ref();
            if matches!(plan.stale, FrameOp::Full) {
                back.copy_from_slice(front);
            }
            for r in plan.stale.regions() {
                copy_region(front, back, width, height, *r);
            }
            for r in plan.changed.regions() {
                convert_rgba_region(rgba, back, width, height, *r, PixelFormat::Argb8888);
            }
            plan.changed
        };
        self.back.age = 1;
        self.back.size = size;
        std::mem::swap(&mut self.front, &mut self.back);
        if self.back.age != 0 {
            self.back.age += 1;
        }
        damage
    }
}

pub(crate) struct WaylandAlphaPresenter {
    _conn: Connection,
    event_queue: EventQueue<State>,
    qh: QueueHandle<State>,
    shm: wl_shm::WlShm,
    surface: wl_surface::WlSurface,
    chain: Option<Swapchain<Buf>>,
}

impl WaylandAlphaPresenter {
    /// Builds a presenter over the app's existing Wayland surface, or `None` when the handles are not Wayland or the connection/globals cannot be set up (the caller then falls back to opaque softbuffer).
    pub(crate) fn try_new(
        display: &impl HasDisplayHandle,
        window: &impl HasWindowHandle,
    ) -> Option<Self> {
        let RawDisplayHandle::Wayland(dh) = display.display_handle().ok()?.as_raw() else {
            return None;
        };
        let RawWindowHandle::Wayland(wh) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: a live `wl_display` owned by the platform layer, valid as long as the window that yielded it, which the caller keeps alive for the renderer's lifetime.
        let backend = unsafe { Backend::from_foreign_display(dh.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, event_queue) = registry_queue_init::<State>(&conn).ok()?;
        let qh = event_queue.handle();
        let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).ok()?;
        // SAFETY: a live `wl_surface` proxy on this same display; wrapping it as an object id mirrors softbuffer.
        let surface_id = unsafe {
            ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                wh.surface.as_ptr().cast(),
            )
        }
        .ok()?;
        let surface = wl_surface::WlSurface::from_id(&conn, surface_id).ok()?;
        Some(Self {
            _conn: conn,
            event_queue,
            qh,
            shm,
            surface,
            chain: None,
        })
    }

    /// Presents a premultiplied-RGBA frame (tiny_skia's pixmap byte order) as premultiplied `Argb8888`, preserving alpha so the compositor blends the surface. `log` must already hold this frame's change, and is advanced only when the frame reaches the compositor.
    pub(crate) fn present(&mut self, rgba: &[u8], width: u32, height: u32, log: &mut PresentLog) {
        let (Ok(w), Ok(h)) = (i32::try_from(width), i32::try_from(height)) else {
            return;
        };
        if w == 0 || h == 0 {
            return;
        }
        let _ = self.event_queue.dispatch_pending(&mut State);

        if self.chain.is_none() {
            let (Ok(front), Ok(back)) = (
                Buf::new(&self.shm, w, h, &self.qh),
                Buf::new(&self.shm, w, h, &self.qh),
            ) else {
                return;
            };
            self.chain = Some(Swapchain::new(front, back));
        }
        let Some(chain) = &mut self.chain else {
            return;
        };

        // Block until the compositor releases the back buffer it last held, then size it to the frame.
        while !chain.back().released.load(Ordering::SeqCst) {
            if self.event_queue.blocking_dispatch(&mut State).is_err() {
                return;
            }
        }
        if chain.back_mut().resize(&self.shm, w, h, &self.qh).is_err() {
            return;
        }

        let damage = {
            let _convert = renderer_core::perf::span(renderer_core::perf::Phase::Convert);
            chain.swap_in(log, rgba, width as usize, height as usize)
        };

        chain.front().attach(&self.surface);
        let damage_buffer = self.surface.version() >= 4;
        let surface_damage = if damage_buffer {
            SurfaceDamage::Rects
        } else {
            SurfaceDamage::AllOrNothing
        };
        let rects = declared_damage(&damage, surface_damage, width, height);
        note_damage(rects.as_deref(), width, height);
        match rects {
            Some(rects) => {
                for r in rects {
                    self.surface.damage_buffer(
                        r.x as i32,
                        r.y as i32,
                        r.width as i32,
                        r.height as i32,
                    );
                }
            }
            None if damage_buffer => self.surface.damage_buffer(0, 0, w, h),
            None => self.surface.damage(0, 0, i32::MAX, i32::MAX),
        }
        self.surface.commit();
        let _ = self.event_queue.flush();
        log.presented();
    }
}

#[cfg(test)]
#[path = "wayland_alpha_test.rs"]
mod tests;
