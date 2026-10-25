//! [`FrameEncoder`] and [`FrameDecoder`]: one frame's draw list across the boundary, with its shared payloads interned across frames and its pixels lent once.

use std::sync::Arc;

use geometry_core::{BorderRadius, Point, Rect};
use rustc_hash::FxHashMap;
use wire_core::{Reader, Wire, WireError, Writer};

use super::intern::{Interner, Slots};
use super::{FrameError, WIRE_VERSION};
use crate::{
    BlendMode, DrawCommand, Element, ImageData, ImageFill, LayerMask, Linked, PathData, PathStyle,
    Raster, RectStyle, Span, Stroke, TextStyle,
};

const MAGIC: [u8; 4] = *b"TLDL";
const FRESH: u8 = 1;

const RECT: u8 = 0;
const TEXT: u8 = 1;
const IMAGE: u8 = 2;
const LINE: u8 = 3;
const PATH: u8 = 4;
const PUSH_CLIP: u8 = 5;
const POP_CLIP: u8 = 6;
const PUSH_MATRIX: u8 = 7;
const POP_MATRIX: u8 = 8;
const PUSH_LAYER: u8 = 9;
const POP_LAYER: u8 = 10;
const PUSH_ELEMENT: u8 = 11;
const POP_ELEMENT: u8 = 12;
const PUSH_SURFACE: u8 = 13;
const POP_SURFACE: u8 = 14;

const PIXELS: u8 = 0;
const LINKED: u8 = 1;
const EXTERNAL: u8 = 2;

/// Declares one table per side of every payload a frame interns, `Sent` the encoder's and `Received` the decoder's, each kept by the frames that still use it.
macro_rules! payloads {
    ($($field:ident: $payload:ty,)*) => {
        #[derive(Default)]
        struct Sent {
            $($field: Interner<$payload>,)*
        }

        impl Sent {
            fn sweep(&mut self, frame: u64) {
                $(self.$field.sweep(frame);)*
            }

            fn clear(&mut self) {
                $(self.$field.clear();)*
            }
        }

        #[derive(Default)]
        struct Received {
            $($field: Slots<$payload>,)*
        }

        impl Received {
            fn sweep(&mut self, frame: u64) {
                $(self.$field.sweep(frame);)*
            }

            fn clear(&mut self) {
                $(self.$field.clear();)*
            }
        }
    };
}

payloads! {
    rect_styles: RectStyle,
    text_styles: TextStyle,
    strokes: Stroke,
    path_styles: PathStyle,
    paths: PathData,
    elements: Element,
    texts: str,
    spans: [Span],
}

/// The guest's half: encodes each frame into a buffer it owns, valid until the next [`encode`](Self::encode), and lends the pixels of the images that frame draws.
///
/// Frames build on each other: a payload the previous frame sent is named rather than sent again, so every frame encoded has to reach the decoder, in order. After one does not, [`reset`](Self::reset) makes the next frame stand alone.
pub struct FrameEncoder {
    out: Writer,
    frame: u64,
    fresh: bool,
    sent: Sent,
    lent: FxHashMap<u64, Arc<ImageData>>,
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self {
            out: Writer::default(),
            frame: 0,
            fresh: true,
            sent: Sent::default(),
            lent: FxHashMap::default(),
        }
    }
}

impl FrameEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn encode(&mut self, commands: &[DrawCommand]) -> &[u8] {
        self.frame += 1;
        let frame = self.frame;
        self.out.clear();
        self.lent.clear();
        if self.fresh {
            self.sent.clear();
        }
        let w = &mut self.out;
        w.raw(&MAGIC);
        w.u16(WIRE_VERSION);
        w.u8(if self.fresh { FRESH } else { 0 });
        w.u64(frame);
        w.len(commands.len());
        for command in commands {
            put_command(w, &mut self.sent, &mut self.lent, frame, command);
        }
        self.sent.sweep(frame);
        self.fresh = false;
        self.out.as_slice()
    }

    #[cfg(test)]
    pub(crate) fn bytes(&self) -> &[u8] {
        self.out.as_slice()
    }

    /// The premultiplied pixels of an image the last frame drew, for the host to copy the first time it meets the image.
    pub fn pixels(&self, id: u64) -> Option<&[u8]> {
        self.lent.get(&id).map(|image| image.pixels())
    }

    pub fn reset(&mut self) {
        self.fresh = true;
    }
}

fn put_command(
    w: &mut Writer,
    sent: &mut Sent,
    lent: &mut FxHashMap<u64, Arc<ImageData>>,
    frame: u64,
    command: &DrawCommand,
) {
    match command {
        DrawCommand::Rect { rect, style } => {
            w.u8(RECT);
            rect.put(w);
            sent.rect_styles
                .put(w, style, frame, |w, style| style.put(w));
        }
        DrawCommand::Text {
            text,
            spans,
            rect,
            style,
        } => {
            w.u8(TEXT);
            sent.texts.put(w, text, frame, |w, text| w.str(text));
            match spans {
                None => w.u8(0),
                Some(spans) => {
                    w.u8(1);
                    sent.spans.put(w, spans, frame, |w, spans| {
                        w.len(spans.len());
                        for span in spans {
                            span.put(w);
                        }
                    });
                }
            }
            rect.put(w);
            sent.text_styles
                .put(w, style, frame, |w, style| style.put(w));
        }
        DrawCommand::Image {
            data,
            rect,
            raster,
            fill,
        } => {
            w.u8(IMAGE);
            put_image(w, lent, data);
            rect.put(w);
            raster.put(w);
            fill.put(w);
        }
        DrawCommand::Line { p1, p2, style } => {
            w.u8(LINE);
            p1.put(w);
            p2.put(w);
            sent.strokes.put(w, style, frame, |w, style| style.put(w));
        }
        DrawCommand::Path { data, style } => {
            w.u8(PATH);
            sent.paths.put(w, data, frame, |w, data| data.put(w));
            sent.path_styles
                .put(w, style, frame, |w, style| style.put(w));
        }
        DrawCommand::PushClip { rect, radius } => {
            w.u8(PUSH_CLIP);
            rect.put(w);
            radius.put(w);
        }
        DrawCommand::PopClip => w.u8(POP_CLIP),
        DrawCommand::PushMatrix { matrix } => {
            w.u8(PUSH_MATRIX);
            matrix.put(w);
        }
        DrawCommand::PopMatrix => w.u8(POP_MATRIX),
        DrawCommand::PushLayer {
            opacity,
            backdrop_blur,
            blend,
            mask,
        } => {
            w.u8(PUSH_LAYER);
            opacity.put(w);
            backdrop_blur.put(w);
            blend.put(w);
            mask.put(w);
        }
        DrawCommand::PopLayer => w.u8(POP_LAYER),
        DrawCommand::PushElement { element } => {
            w.u8(PUSH_ELEMENT);
            sent.elements
                .put(w, element, frame, |w, element| element.put(w));
        }
        DrawCommand::PopElement => w.u8(POP_ELEMENT),
        DrawCommand::PushSurface { surface } => {
            w.u8(PUSH_SURFACE);
            surface.put(w);
        }
        DrawCommand::PopSurface => w.u8(POP_SURFACE),
    }
}

fn put_image(w: &mut Writer, lent: &mut FxHashMap<u64, Arc<ImageData>>, image: &Arc<ImageData>) {
    let kind = if image.linked_source().is_some() {
        LINKED
    } else if image.external_texture().is_some() {
        EXTERNAL
    } else {
        lent.entry(image.id).or_insert_with(|| Arc::clone(image));
        PIXELS
    };
    w.u8(kind);
    image.id.put(w);
    image.width.put(w);
    image.height.put(w);
    if let Some(linked) = image.linked_source() {
        linked.put(w);
    }
}

/// The host's half: decodes a lent frame into commands the host allocated, keeping what consecutive frames share so an unchanged payload decodes to the same `Arc` frame after frame.
///
/// Images are kept by content id and copied from the guest only the first time a frame draws one; an external texture the guest made cannot cross, and decodes as an image with no pixels.
#[derive(Default)]
pub struct FrameDecoder {
    last: Option<u64>,
    received: Received,
    images: FxHashMap<u64, (Arc<ImageData>, u64)>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces `out` with the frame in `bytes`. `copy_pixels` copies the pixels of an image the host has not seen, from [`FrameEncoder::pixels`] across the boundary. After an error `out` is empty, and the guest's encoder has to be [`reset`](FrameEncoder::reset).
    pub fn decode(
        &mut self,
        bytes: &[u8],
        out: &mut Vec<DrawCommand>,
        mut copy_pixels: impl FnMut(u64) -> Option<Vec<u8>>,
    ) -> Result<(), FrameError> {
        out.clear();
        let decoded = self.decode_frame(bytes, out, &mut copy_pixels);
        if decoded.is_err() {
            out.clear();
            self.last = None;
            self.received.clear();
        }
        decoded
    }

    fn decode_frame(
        &mut self,
        bytes: &[u8],
        out: &mut Vec<DrawCommand>,
        copy_pixels: &mut dyn FnMut(u64) -> Option<Vec<u8>>,
    ) -> Result<(), FrameError> {
        let mut r = Reader::new(bytes);
        if r.raw(MAGIC.len()).map_err(|_| FrameError::NotAFrame)? != MAGIC {
            return Err(FrameError::NotAFrame);
        }
        let version = r.u16()?;
        if version != WIRE_VERSION {
            return Err(FrameError::Version { found: version });
        }
        let flags = r.u8()?;
        if flags & !FRESH != 0 {
            return Err(WireError::Invalid("frame flags").into());
        }
        let frame = r.u64()?;
        if flags & FRESH != 0 {
            self.received.clear();
        } else if self.last.and_then(|last| last.checked_add(1)) != Some(frame) {
            return Err(FrameError::OutOfStep {
                last: self.last,
                found: frame,
            });
        }
        let count = r.len()?;
        out.reserve(count);
        let mut images = Images {
            known: &mut self.images,
            copy_pixels,
            frame,
        };
        for _ in 0..count {
            out.push(take_command(
                &mut r,
                &mut self.received,
                &mut images,
                frame,
            )?);
        }
        if !r.is_empty() {
            return Err(WireError::Trailing(r.remaining()).into());
        }
        self.received.sweep(frame);
        self.images.retain(|_, (_, used)| *used == frame);
        self.last = Some(frame);
        Ok(())
    }
}

struct Images<'a> {
    known: &'a mut FxHashMap<u64, (Arc<ImageData>, u64)>,
    copy_pixels: &'a mut dyn FnMut(u64) -> Option<Vec<u8>>,
    frame: u64,
}

impl Images<'_> {
    fn take(&mut self, r: &mut Reader<'_>) -> Result<Arc<ImageData>, FrameError> {
        let kind = r.u8()?;
        let id = u64::take(r)?;
        let width = u32::take(r)?;
        let height = u32::take(r)?;
        let linked = match kind {
            LINKED => Some(Linked::take(r)?),
            PIXELS | EXTERNAL => None,
            tag => return Err(WireError::Tag { of: "image", tag }.into()),
        };
        if let Some((image, used)) = self.known.get_mut(&id) {
            *used = self.frame;
            return Ok(Arc::clone(image));
        }
        let image = match (kind, linked) {
            (LINKED, Some(linked)) => {
                let variants: Vec<(u32, &str)> = linked
                    .variants
                    .iter()
                    .map(|(width, url)| (*width, &**url))
                    .collect();
                ImageData::linked(&linked.url, width, height, &variants)
            }
            (EXTERNAL, _) => ImageData::without_pixels(id, width, height),
            _ => self.image_with_pixels(id, width, height)?,
        };
        if image.id != id {
            return Err(FrameError::ImageMismatch(id));
        }
        let image = Arc::new(image);
        self.known.insert(id, (Arc::clone(&image), self.frame));
        Ok(image)
    }

    fn image_with_pixels(
        &mut self,
        id: u64,
        width: u32,
        height: u32,
    ) -> Result<ImageData, FrameError> {
        let pixels = (self.copy_pixels)(id).ok_or(FrameError::MissingImage(id))?;
        let expected = u64::from(width) * u64::from(height) * 4;
        if expected > u64::from(u32::MAX) || pixels.len() as u64 != expected {
            return Err(WireError::Invalid("lent pixels do not fill the image").into());
        }
        Ok(ImageData::from_premultiplied(pixels, width, height))
    }
}

fn take_command(
    r: &mut Reader<'_>,
    received: &mut Received,
    images: &mut Images<'_>,
    frame: u64,
) -> Result<DrawCommand, FrameError> {
    Ok(match r.u8()? {
        RECT => {
            let rect = Rect::take(r)?;
            let style = received.rect_styles.take(r, frame, Wire::take)?;
            DrawCommand::Rect { rect, style }
        }
        TEXT => {
            let text = received
                .texts
                .take(r, frame, |r| r.str().map(Arc::<str>::from))?;
            let spans = match r.u8()? {
                0 => None,
                1 => Some(received.spans.take(r, frame, Wire::take)?),
                tag => return Err(WireError::Tag { of: "spans", tag }.into()),
            };
            let rect = Rect::take(r)?;
            let style = received.text_styles.take(r, frame, Wire::take)?;
            DrawCommand::Text {
                text,
                spans,
                rect,
                style,
            }
        }
        IMAGE => {
            let data = images.take(r)?;
            let rect = Rect::take(r)?;
            let raster = Raster::take(r)?;
            let fill = ImageFill::take(r)?;
            DrawCommand::Image {
                data,
                rect,
                raster,
                fill,
            }
        }
        LINE => {
            let p1 = Point::take(r)?;
            let p2 = Point::take(r)?;
            let style = received.strokes.take(r, frame, Wire::take)?;
            DrawCommand::Line { p1, p2, style }
        }
        PATH => {
            let data = received.paths.take(r, frame, Wire::take)?;
            let style = received.path_styles.take(r, frame, Wire::take)?;
            DrawCommand::Path { data, style }
        }
        PUSH_CLIP => {
            let rect = Rect::take(r)?;
            let radius = BorderRadius::take(r)?;
            DrawCommand::PushClip { rect, radius }
        }
        POP_CLIP => DrawCommand::PopClip,
        PUSH_MATRIX => DrawCommand::PushMatrix {
            matrix: Wire::take(r)?,
        },
        POP_MATRIX => DrawCommand::PopMatrix,
        PUSH_LAYER => {
            let opacity = f32::take(r)?;
            let backdrop_blur = f32::take(r)?;
            let blend = BlendMode::take(r)?;
            let mask = LayerMask::take(r)?;
            DrawCommand::PushLayer {
                opacity,
                backdrop_blur,
                blend,
                mask,
            }
        }
        POP_LAYER => DrawCommand::PopLayer,
        PUSH_ELEMENT => DrawCommand::PushElement {
            element: received.elements.take(r, frame, Wire::take)?,
        },
        POP_ELEMENT => DrawCommand::PopElement,
        PUSH_SURFACE => DrawCommand::PushSurface {
            surface: u64::take(r)?,
        },
        POP_SURFACE => DrawCommand::PopSurface,
        tag => return Err(WireError::Tag { of: "command", tag }.into()),
    })
}

#[cfg(test)]
#[path = "frame_test.rs"]
mod tests;
