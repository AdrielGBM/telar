//! [`Frame`]: one picture of a canvas, kept to look at, compare or check after the canvas has moved on.

use std::fmt;
use std::sync::Arc;

use ui_core::accessibility::{self, FrameReading};
use ui_core::focus::{self, TabStop};

use crate::{AccessNode, Color, Direction, DrawCommand, Size, SurfaceCanvas};

/// What a canvas drew at one moment, in its own coordinates, with the environment it drew it in.
///
/// Cheap to clone: the commands are shared. A panel shows one by drawing its commands at its size, which is how a play's steps keep the picture each one left behind.
#[non_exhaustive]
#[derive(Clone)]
pub struct Frame {
    pub commands: Arc<[DrawCommand]>,
    /// The canvas size, in logical px.
    pub size: Size,
    pub direction: Direction,
    /// The language the canvas is in, as a BCP 47 tag. `None` where it has none.
    pub lang: Option<String>,
    /// What shows behind everything the commands draw: the window's clear colour. `None` where it is not known.
    pub background: Option<Color>,
    /// What a reader and the keyboard were told about the frame when it was drawn. `None` for a frame made from commands alone.
    pub access: Option<Arc<FrameAccess>>,
}

/// What a reader and the keyboard are told about one frame: the surface's live state at that moment, kept with the frame so it can be checked once the surface has moved on.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameAccess {
    /// What a reader is told, in reading order: the [`accessibility::snapshot`].
    pub snapshot: Vec<AccessNode>,
    /// What a reader skips, and what it calls each box and picture.
    pub reading: FrameReading,
    /// The stops Tab walks, in the order it walks them.
    pub tab_order: Vec<TabStop>,
}

impl FrameAccess {
    /// What the surface entered now tells a reader and the keyboard about `commands`, its own frame.
    pub fn read(commands: &[DrawCommand]) -> Self {
        Self {
            snapshot: accessibility::snapshot(commands),
            reading: FrameReading::of(commands),
            tab_order: focus::tab_order(),
        }
    }
}

impl Frame {
    pub fn new(commands: impl Into<Arc<[DrawCommand]>>, size: Size) -> Self {
        Self {
            commands: commands.into(),
            size,
            direction: Direction::Ltr,
            lang: None,
            background: None,
            access: None,
        }
    }

    /// The frame `canvas` shows now, in the direction and language it lays out and translates in, with what a reader and the keyboard are told about it.
    pub fn capture(canvas: &SurfaceCanvas) -> Self {
        let commands: Arc<[DrawCommand]> = canvas.frame_commands().iter().cloned().collect();
        let size = canvas.size();
        let _entered = canvas.enter();
        let access = FrameAccess::read(&commands);
        Self::new(commands, size)
            .with_direction(crate::current_direction())
            .with_lang(i18n_core::current_locale())
            .with_access(Some(access))
    }

    pub fn with_direction(self, direction: Direction) -> Self {
        Self { direction, ..self }
    }

    pub fn with_lang(self, lang: Option<String>) -> Self {
        Self { lang, ..self }
    }

    pub fn with_background(self, background: Option<Color>) -> Self {
        Self { background, ..self }
    }

    pub fn with_access(self, access: Option<FrameAccess>) -> Self {
        Self {
            access: access.map(Arc::new),
            ..self
        }
    }

    /// Every string the frame draws, in draw order.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.commands.iter().filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(&**text),
            _ => None,
        })
    }

    /// The frame drawn at one pixel per logical pixel on the CPU rasterizer, over its background or over nothing, encoded as a PNG.
    ///
    /// Text sets in whatever faces the process has loaded. Drawn on the thread's one [`rasterize`](crate::rasterize) renderer.
    #[cfg(any(feature = "preview-headless", feature = "preview-snapshots"))]
    pub fn to_png(&self) -> Result<Vec<u8>, String> {
        crate::raster::rasterize_png(
            &self.commands,
            pixels(self.size.width),
            pixels(self.size.height),
            Some(self.background.unwrap_or(Color::TRANSPARENT)),
        )
    }
}

/// A logical length as whole device pixels at one pixel per logical pixel, never less than one.
#[cfg(any(feature = "preview-headless", feature = "preview-snapshots"))]
pub(crate) fn pixels(logical: f32) -> u32 {
    if logical.is_finite() {
        logical.round().max(1.0) as u32
    } else {
        1
    }
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("commands", &self.commands.len())
            .field("size", &self.size)
            .field("direction", &self.direction)
            .field("lang", &self.lang)
            .field("background", &self.background)
            .field("access", &self.access.is_some())
            .finish()
    }
}
