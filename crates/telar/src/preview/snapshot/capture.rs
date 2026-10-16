//! Capturing a snapshot: a canvas's frame drawn on the CPU rasterizer and written as draw text.

use std::fmt::Write as _;

use crate::Direction;

use super::super::frame::pixels;
use super::super::{Frame, MatrixCell, Play, PlayError, PreviewEntry};
use super::compare::Image;
use super::{SnapshotError, SnapshotKey};

/// One preview, or one cell of its matrix, at one moment of its play, as a snapshot keeps it: a picture and the draw commands behind it as text.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// What it is of, and what both golden files are named by.
    pub key: SnapshotKey,
    /// The picture, encoded as a PNG.
    pub png: Vec<u8>,
    /// The frame as [`renderer_record::draw_text`] writes it, under a `frame` line naming its size, direction, language and background.
    pub draw: String,
    pub(crate) image: Image,
}

impl Snapshot {
    /// `frame` drawn at one pixel per logical pixel on the CPU rasterizer, over its background or over nothing, and written as text, as the snapshot `key` names.
    ///
    /// Text sets in whatever faces the process has loaded: [`Snapshots`](super::Snapshots) is what makes those the declared ones alone before anything is captured.
    pub fn of_frame(key: SnapshotKey, frame: &Frame) -> Result<Self, SnapshotError> {
        let png = frame.to_png().map_err(SnapshotError::Render)?;
        // Read back from the PNG rather than the rasterizer, so the picture compared is the one a golden written from it would hold.
        let image = Image::decode(&png).map_err(SnapshotError::Render)?;
        Ok(Self {
            key,
            png,
            draw: draw_text(frame),
            image,
        })
    }
}

/// Mounts `entry` as a workshop canvas would, in `cell` when there is one, and runs its play: the canvas as it opened, settled, and as the play left it, or why the play stopped before its end.
pub(super) fn open_and_play(
    entry: &PreviewEntry,
    cell: Option<&MatrixCell>,
) -> Result<(Frame, Result<Frame, PlayError>), SnapshotError> {
    let mut play = match cell {
        None => Play::mount(entry),
        Some(cell) => {
            let ctx = cell
                .ctx(entry)
                .map_err(|error| SnapshotError::Cell(error.to_string()))?;
            Play::mount_with(entry, ctx, Play::DEFAULT_VIEWPORT)
        }
    }
    .map_err(SnapshotError::Mount)?;
    let opened = play.frame();
    let ended = play.run().map(|()| play.frame());
    Ok((opened, ended))
}

fn draw_text(frame: &Frame) -> String {
    let mut text = format!(
        "frame {}x{}",
        pixels(frame.size.width),
        pixels(frame.size.height)
    );
    if frame.direction == Direction::Rtl {
        text.push_str(" dir=rtl");
    }
    if let Some(lang) = &frame.lang {
        let _ = write!(text, " lang={lang}");
    }
    if let Some(background) = frame.background {
        let [r, g, b, a] = background.to_rgba8();
        let _ = write!(text, " background=#{r:02x}{g:02x}{b:02x}{a:02x}");
    }
    text.push('\n');
    text.push_str(&renderer_record::draw_text(&frame.commands));
    text
}
