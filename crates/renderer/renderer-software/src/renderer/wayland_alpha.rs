//! A Wayland `wl_shm` `Argb8888` present path for the software renderer, used when the app asks for a
//! transparent surface.
//!
//! softbuffer only offers opaque `Xrgb8888`, so this bypasses it and manages its own alpha-preserving shm
//! buffers — mirroring softbuffer's own Wayland backend (from which the shm/pool/release plumbing is adapted)
//! and the Android `ANativeWindow` bypass, so all three present paths honor transparency consistently. The
//! connection is built from the *foreign* display pointer, so it shares the app's existing Wayland display;
//! present runs on the same thread as the surface's event loop, exactly like the softbuffer path it replaces.

use std::fs::File;
use std::os::fd::{AsFd, AsRawFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use memmap2::MmapMut;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_buffer, wl_registry, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};

/// The event-dispatch sink. Only `wl_buffer.release` carries state (flips the buffer's `released` flag); the
/// rest are inert because we drive everything with explicit requests.
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

fn create_memfile() -> std::io::Result<File> {
    use rustix::fs::{MemfdFlags, SealFlags};
    let fd = rustix::fs::memfd_create(
        c"telar-alpha-shm",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )?;
    // Sealing lets the compositor mmap the fd read-only without worrying it might shrink under it.
    let _ = rustix::fs::fcntl_add_seals(&fd, SealFlags::SHRINK | SealFlags::SEAL);
    Ok(File::from(fd))
}

fn pool_size(width: i32, height: i32) -> i32 {
    ((width.max(1) * height.max(1) * 4) as u32).next_power_of_two() as i32
}

/// One `wl_shm` `Argb8888` buffer plus its mmap'd backing store. Double-buffered by the presenter so the
/// compositor can hold one while the next is filled.
struct Buf {
    tempfile: File,
    map: MmapMut,
    pool: wl_shm_pool::WlShmPool,
    pool_size: i32,
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
    ) -> std::io::Result<Self> {
        let size = pool_size(width, height);
        let tempfile = create_memfile()?;
        tempfile.set_len(size as u64)?;
        let map = unsafe { MmapMut::map_mut(tempfile.as_raw_fd())? };
        let pool = shm.create_pool(tempfile.as_fd(), size, qh, ());
        let released = Arc::new(AtomicBool::new(true));
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Argb8888,
            qh,
            released.clone(),
        );
        Ok(Self {
            tempfile,
            map,
            pool,
            pool_size: size,
            buffer,
            width,
            height,
            released,
        })
    }

    fn resize(&mut self, width: i32, height: i32, qh: &QueueHandle<State>) -> std::io::Result<()> {
        if self.width == width && self.height == height {
            return Ok(());
        }
        self.buffer.destroy();
        let size = pool_size(width, height);
        if size > self.pool_size {
            self.tempfile.set_len(size as u64)?;
            self.pool.resize(size);
            self.pool_size = size;
            self.map = unsafe { MmapMut::map_mut(self.tempfile.as_raw_fd())? };
        }
        self.buffer = self.pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Argb8888,
            qh,
            self.released.clone(),
        );
        self.width = width;
        self.height = height;
        Ok(())
    }

    fn attach(&self, surface: &wl_surface::WlSurface) {
        self.released.store(false, Ordering::SeqCst);
        surface.attach(Some(&self.buffer), 0, 0);
    }

    fn pixels_mut(&mut self) -> &mut [u32] {
        let len = self.width.max(0) as usize * self.height.max(0) as usize;
        unsafe { std::slice::from_raw_parts_mut(self.map.as_mut_ptr() as *mut u32, len) }
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        self.buffer.destroy();
        self.pool.destroy();
    }
}

pub(crate) struct WaylandAlphaPresenter {
    _conn: Connection,
    event_queue: EventQueue<State>,
    qh: QueueHandle<State>,
    shm: wl_shm::WlShm,
    surface: wl_surface::WlSurface,
    buffers: Option<(Buf, Buf)>,
}

impl WaylandAlphaPresenter {
    /// Builds a presenter over the app's existing Wayland surface, or `None` when the handles are not Wayland
    /// or the connection/globals cannot be set up (the caller then falls back to opaque softbuffer).
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
        // SAFETY: the display pointer is a live `wl_display` owned by the app's platform layer, valid for as long as the window that yielded it; the caller keeps that window alive for the renderer's lifetime.
        let backend = unsafe { Backend::from_foreign_display(dh.display.as_ptr().cast()) };
        let conn = Connection::from_backend(backend);
        let (globals, event_queue) = registry_queue_init::<State>(&conn).ok()?;
        let qh = event_queue.handle();
        let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).ok()?;
        // SAFETY: `wh.surface` is a live `wl_surface` proxy on this same display; wrapping it as an object id on the shared backend mirrors softbuffer.
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
            buffers: None,
        })
    }

    /// Presents a premultiplied-RGBA frame (tiny_skia's pixmap byte order) as premultiplied `Argb8888`,
    /// preserving alpha so the compositor blends the surface.
    pub(crate) fn present(&mut self, rgba: &[u8], width: u32, height: u32) {
        let (w, h) = (width as i32, height as i32);
        if w <= 0 || h <= 0 {
            return;
        }
        let _ = self.event_queue.dispatch_pending(&mut State);

        if self.buffers.is_none() {
            let (Ok(front), Ok(back)) = (
                Buf::new(&self.shm, w, h, &self.qh),
                Buf::new(&self.shm, w, h, &self.qh),
            ) else {
                return;
            };
            self.buffers = Some((front, back));
        }

        // Block until the back buffer the compositor last held is released, then size it to the frame.
        let released = self.buffers.as_ref().unwrap().1.released.clone();
        while !released.load(Ordering::SeqCst) {
            if self.event_queue.blocking_dispatch(&mut State).is_err() {
                return;
            }
        }
        let back = &mut self.buffers.as_mut().unwrap().1;
        if back.resize(w, h, &self.qh).is_err() {
            return;
        }

        // tiny_skia gives premultiplied RGBA bytes; ARGB8888 shm on little-endian wants the u32 0xAARRGGBB (also premultiplied), so pack per pixel.
        let dst = back.pixels_mut();
        let n = dst.len().min(rgba.len() / 4);
        for (i, px) in dst[..n].iter_mut().enumerate() {
            let r = rgba[i * 4] as u32;
            let g = rgba[i * 4 + 1] as u32;
            let b = rgba[i * 4 + 2] as u32;
            let a = rgba[i * 4 + 3] as u32;
            *px = (a << 24) | (r << 16) | (g << 8) | b;
        }

        let (front, back) = self.buffers.as_mut().unwrap();
        std::mem::swap(front, back);
        front.attach(&self.surface);
        if self.surface.version() >= 4 {
            self.surface.damage_buffer(0, 0, w, h);
        } else {
            self.surface.damage(0, 0, i32::MAX, i32::MAX);
        }
        self.surface.commit();
        let _ = self.event_queue.flush();
    }
}
