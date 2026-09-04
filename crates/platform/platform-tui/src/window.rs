//! The terminal as a window.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use platform_core::Window;

/// The terminal's size and redraw state, shared with whatever holds a clone of the window.
struct Inner {
    cols: AtomicU32,
    rows: AtomicU32,
    cell_width: f32,
    cell_height: f32,
    redraw: AtomicBool,
}

/// A terminal, presented as a window whose size is expressed in the logical pixels layout works in.
///
/// The exchange rate is the cell size the renderer was built with: an 80×24 terminal at the default 8×16 is a 640×384 window. Nothing above this learns that a cell exists, which is what lets the same widget tree lay out here and on a desktop.
#[derive(Clone)]
pub struct TuiWindow {
    inner: Arc<Inner>,
}

impl TuiWindow {
    pub fn new(cols: u16, rows: u16, cell_width: f32, cell_height: f32) -> Self {
        Self {
            inner: Arc::new(Inner {
                cols: AtomicU32::new(cols as u32),
                rows: AtomicU32::new(rows as u32),
                cell_width,
                cell_height,
                redraw: AtomicBool::new(true),
            }),
        }
    }

    pub fn set_grid(&self, cols: u16, rows: u16) {
        self.inner.cols.store(cols as u32, Ordering::Relaxed);
        self.inner.rows.store(rows as u32, Ordering::Relaxed);
        self.inner.redraw.store(true, Ordering::Relaxed);
    }

    pub fn grid(&self) -> (u16, u16) {
        (
            self.inner.cols.load(Ordering::Relaxed) as u16,
            self.inner.rows.load(Ordering::Relaxed) as u16,
        )
    }

    pub fn cell(&self) -> (f32, f32) {
        (self.inner.cell_width, self.inner.cell_height)
    }

    /// Whether a redraw was asked for since this was last called, clearing the request.
    pub fn take_redraw_request(&self) -> bool {
        self.inner.redraw.swap(false, Ordering::Relaxed)
    }
}

impl Window for TuiWindow {
    fn redraw_waker(&self) -> Option<std::sync::Arc<dyn Fn() + Send + Sync>> {
        Some(platform_core::window_waker(self))
    }

    fn width(&self) -> u32 {
        (self.inner.cols.load(Ordering::Relaxed) as f32 * self.inner.cell_width).round() as u32
    }

    fn height(&self) -> u32 {
        (self.inner.rows.load(Ordering::Relaxed) as f32 * self.inner.cell_height).round() as u32
    }

    fn request_redraw(&self) {
        self.inner.redraw.store(true, Ordering::Relaxed);
    }

    /// One, always. A cell is the smallest thing a terminal can address, and the window already reports its size in the units layout uses — there is no second grid underneath to scale into.
    fn scale_factor(&self) -> f64 {
        1.0
    }

    fn set_title(&self, title: &str) {
        // OSC 0 sets both the icon name and the window title, which is what a terminal emulator shows in its tab. A terminal that does not understand it ignores the sequence rather than printing it.
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = write!(out, "\x1b]0;{title}\x07");
        let _ = out.flush();
    }

    /// What `COLORFGBG` says, which is the only light/dark signal a terminal offers without a round trip. Its second field is the background's palette index: the dark half of the 16 colours, plus 8 (grey), means a dark background.
    fn prefers_dark(&self) -> Option<bool> {
        let value = std::env::var("COLORFGBG").ok()?;
        let background = value.rsplit(';').next()?.trim();
        let index: u8 = background.parse().ok()?;
        Some(matches!(index, 0..=6 | 8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_is_reported_in_logical_pixels() {
        let w = TuiWindow::new(80, 24, 8.0, 16.0);
        assert_eq!(w.width(), 640);
        assert_eq!(w.height(), 384);
    }

    #[test]
    fn a_resize_is_visible_through_a_clone() {
        let w = TuiWindow::new(80, 24, 8.0, 16.0);
        let clone = w.clone();
        w.set_grid(100, 30);
        assert_eq!(clone.width(), 800);
    }

    #[test]
    fn a_redraw_request_is_taken_once() {
        let w = TuiWindow::new(80, 24, 8.0, 16.0);
        assert!(
            w.take_redraw_request(),
            "a fresh window owes its first frame"
        );
        assert!(!w.take_redraw_request());
        w.request_redraw();
        assert!(w.take_redraw_request());
    }
}
