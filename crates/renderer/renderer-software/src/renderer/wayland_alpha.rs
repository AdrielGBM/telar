//! softbuffer offers only opaque `Xrgb8888`, so a transparent surface gets shm buffers of its own: `Abgr8888` where the compositor advertises it, which tiny-skia draws into directly, and converted `Argb8888` elsewhere.

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
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};

use super::present::{FrameOp, SurfaceDamage, declared_damage, note_damage};
use super::swapchain::{ShmBuffer, ShmLayout, ShmPresenter, Wire};

/// The event-dispatch sink. `wl_shm.format` fills in which layouts the compositor takes and `wl_buffer.release` flips the buffer's `released` flag; the rest are inert because we drive everything with explicit requests.
#[derive(Default)]
struct State {
    formats: Vec<wl_shm::Format>,
}

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
        state: &mut State,
        _: &wl_shm::WlShm,
        event: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let wl_shm::Event::Format {
            format: WEnum::Value(format),
        } = event
        {
            state.formats.push(format);
        }
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

// The layout to draw in, from the formats the compositor advertised. `Argb8888` is one every compositor must take.
fn layout_for(formats: &[wl_shm::Format]) -> ShmLayout {
    if formats.contains(&wl_shm::Format::Abgr8888) {
        ShmLayout::Rgba
    } else {
        ShmLayout::Argb
    }
}

fn shm_format(layout: ShmLayout) -> wl_shm::Format {
    match layout {
        ShmLayout::Rgba => wl_shm::Format::Abgr8888,
        ShmLayout::Argb => wl_shm::Format::Argb8888,
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
    format: wl_shm::Format,
    qh: &QueueHandle<State>,
    released: &Arc<AtomicBool>,
) -> wl_buffer::WlBuffer {
    pool.create_buffer(0, width, height, width * 4, format, qh, released.clone())
}

pub(super) struct Buf {
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
        format: wl_shm::Format,
        qh: &QueueHandle<State>,
    ) -> io::Result<Self> {
        let size = frame_bytes(width, height)?;
        let file = ShmFile::new(size)?;
        let pool = shm.create_pool(file.fd(), size, qh, ());
        let released = Arc::new(AtomicBool::new(true));
        let buffer = create_buffer(&pool, width, height, format, qh, &released);
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
        format: wl_shm::Format,
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
        let buffer = create_buffer(&self.pool, width, height, format, qh, &self.released);
        std::mem::replace(&mut self.buffer, buffer).destroy();
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

impl ShmBuffer for Buf {
    fn pixels(&self) -> &[u32] {
        self.file.pixels(self.pixel_count())
    }

    fn pixels_mut(&mut self) -> &mut [u32] {
        let count = self.pixel_count();
        self.file.pixels_mut(count)
    }

    fn released(&self) -> bool {
        self.released.load(Ordering::SeqCst)
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
    }
}

pub(super) struct WaylandWire {
    _conn: Connection,
    event_queue: EventQueue<State>,
    state: State,
    qh: QueueHandle<State>,
    shm: wl_shm::WlShm,
    format: wl_shm::Format,
    surface: wl_surface::WlSurface,
}

impl Wire for WaylandWire {
    type Buffer = Buf;

    fn create(&mut self, width: u32, height: u32) -> Option<Buf> {
        let (width, height) = (i32::try_from(width).ok()?, i32::try_from(height).ok()?);
        Buf::new(&self.shm, width, height, self.format, &self.qh).ok()
    }

    fn resize(&mut self, buffer: &mut Buf, width: u32, height: u32) -> bool {
        let (Ok(width), Ok(height)) = (i32::try_from(width), i32::try_from(height)) else {
            return false;
        };
        buffer
            .resize(&self.shm, width, height, self.format, &self.qh)
            .is_ok()
    }

    fn dispatch_pending(&mut self) {
        let _ = self.event_queue.dispatch_pending(&mut self.state);
    }

    fn wait(&mut self) -> bool {
        self.event_queue.blocking_dispatch(&mut self.state).is_ok()
    }

    fn commit(&mut self, buffer: &mut Buf, changed: &FrameOp, width: u32, height: u32) {
        buffer.released.store(false, Ordering::SeqCst);
        self.surface.attach(Some(&buffer.buffer), 0, 0);
        let damage_buffer = self.surface.version() >= 4;
        let surface_damage = if damage_buffer {
            SurfaceDamage::Rects
        } else {
            SurfaceDamage::AllOrNothing
        };
        let rects = declared_damage(changed, surface_damage, width, height);
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
            None if damage_buffer => self
                .surface
                .damage_buffer(0, 0, buffer.width, buffer.height),
            None => self.surface.damage(0, 0, i32::MAX, i32::MAX),
        }
        self.surface.commit();
        let _ = self.event_queue.flush();
    }
}

/// Builds a presenter over the app's existing Wayland surface, or `None` when the handles are not Wayland or the connection/globals cannot be set up (the caller then falls back to opaque softbuffer).
pub(super) fn try_new(
    display: &impl HasDisplayHandle,
    window: &impl HasWindowHandle,
) -> Option<ShmPresenter<WaylandWire>> {
    let RawDisplayHandle::Wayland(dh) = display.display_handle().ok()?.as_raw() else {
        return None;
    };
    let RawWindowHandle::Wayland(wh) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: a live `wl_display` owned by the platform layer, valid as long as the window that yielded it, which the caller keeps alive for the renderer's lifetime.
    let backend = unsafe { Backend::from_foreign_display(dh.display.as_ptr().cast()) };
    let conn = Connection::from_backend(backend);
    let (globals, mut event_queue) = registry_queue_init::<State>(&conn).ok()?;
    let qh = event_queue.handle();
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).ok()?;
    // The formats arrive as events right after the bind.
    let mut state = State::default();
    event_queue.roundtrip(&mut state).ok()?;
    let layout = layout_for(&state.formats);
    tracing::debug!(?layout, formats = ?state.formats, "software renderer: wl_shm layout");
    // SAFETY: a live `wl_surface` proxy on this same display; wrapping it as an object id mirrors softbuffer.
    let surface_id = unsafe {
        ObjectId::from_ptr(
            wl_surface::WlSurface::interface(),
            wh.surface.as_ptr().cast(),
        )
    }
    .ok()?;
    let surface = wl_surface::WlSurface::from_id(&conn, surface_id).ok()?;
    let wire = WaylandWire {
        _conn: conn,
        event_queue,
        state,
        qh,
        shm,
        format: shm_format(layout),
        surface,
    };
    Some(ShmPresenter::new(wire, layout))
}

#[cfg(test)]
#[path = "wayland_alpha_test.rs"]
mod tests;
