//! Capturing a snapshot: a canvas's frame drawn on the CPU rasterizer and written as draw text.

use std::fmt::Write as _;

use crate::Direction;

use super::super::frame::pixels;
use super::super::{Frame, MatrixCell, Play, PreviewEntry};
use super::SnapshotError;
use super::compare::Image;

/// One preview, or one cell of its matrix, as a snapshot keeps it: a picture and the draw commands behind it as text.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct Snapshot {
    /// What both golden files are named by: the preview's id, or the cell's.
    pub id: String,
    /// The id of the preview this is of.
    pub preview: String,
    /// The cell's axis and value pairs, in axis order; empty for the preview itself.
    pub cell: Vec<(String, String)>,
    /// The picture, encoded as a PNG.
    pub png: Vec<u8>,
    /// The frame as [`renderer_record::draw_text`] writes it, under a `frame` line naming its size, direction, language and background.
    pub draw: String,
    pub(crate) image: Image,
}

impl Snapshot {
    /// `frame` drawn at one pixel per logical pixel on the CPU rasterizer, over its background or over nothing, and written as text.
    ///
    /// Text sets in whatever faces the process has loaded: [`Snapshots`](super::Snapshots) is what makes those the declared ones alone before anything is captured.
    pub fn of_frame(id: impl Into<String>, frame: &Frame) -> Result<Self, SnapshotError> {
        let id = id.into();
        let png = frame.to_png().map_err(SnapshotError::Render)?;
        // Read back from the PNG rather than the rasterizer, so the picture compared is the one a golden written from it would hold.
        let image = Image::decode(&png).map_err(SnapshotError::Render)?;
        Ok(Self {
            preview: id.clone(),
            id,
            cell: Vec::new(),
            png,
            draw: draw_text(frame),
            image,
        })
    }

    /// The same snapshot, recorded as `preview`'s, in the matrix cell `cell` names.
    pub fn of_preview(self, preview: impl Into<String>, cell: Vec<(String, String)>) -> Self {
        Self {
            preview: preview.into(),
            cell,
            ..self
        }
    }
}

/// Mounts `entry` as a workshop canvas would, in `cell` when there is one, and snapshots the canvas as it opens: settled, before any play.
pub(super) fn capture(
    entry: &PreviewEntry,
    cell: Option<&MatrixCell>,
) -> Result<Snapshot, SnapshotError> {
    let play = match cell {
        None => Play::mount(entry),
        Some(cell) => {
            let ctx = cell
                .ctx(entry)
                .map_err(|error| SnapshotError::Cell(error.to_string()))?;
            Play::mount_with(entry, ctx, Play::DEFAULT_VIEWPORT)
        }
    }
    .map_err(SnapshotError::Mount)?;
    let frame = play.frame();
    let id = cell.map_or(entry.id, |cell| &cell.id);
    let coordinates = cell.map_or_else(Vec::new, |cell| {
        cell.coordinates
            .iter()
            .map(|(axis, value)| (axis.to_string(), value.clone()))
            .collect()
    });
    Ok(Snapshot::of_frame(id, &frame)?.of_preview(entry.id, coordinates))
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
