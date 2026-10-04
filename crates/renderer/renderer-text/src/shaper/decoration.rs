//! Where the lines a style draws along its glyphs fall, measured from the shaped glyphs so each backend draws exactly what it shapes.

use cosmic_text::{LayoutGlyph, fontdb};
use geometry_core::Rect;
use renderer_core::{DecorationLine, Paint, Span, TextStyle};

use super::{TextShaper, make_cased_buffer, styled_runs};

/// One line drawn along a stretch of glyphs: where it lies, in the coordinates of the rect the text was laid into, and its paint.
#[derive(Debug, Clone, PartialEq)]
pub struct DecorationStroke {
    pub rect: Rect,
    pub paint: Paint,
}

/// A face's own underline, as fractions of its em: how far below the baseline its top edge sits, and how thick it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct UnderlineMetrics {
    offset: f32,
    thickness: f32,
}

/// Faces that carry no `post` metrics still get an underline that reads as one.
const FALLBACK_UNDERLINE: UnderlineMetrics = UnderlineMetrics {
    offset: 0.1,
    thickness: 0.06,
};

impl TextShaper {
    /// The underlines `text` laid into `rect` draws: one per line per stretch of glyphs whose style asks for one, placed from the baseline the glyphs sit on and snapped to the pixel grid `scale_factor` makes, as a document snaps them. Empty, without shaping anything, when nothing in the paragraph is underlined.
    pub fn decorations(
        &mut self,
        text: &str,
        spans: Option<&[Span]>,
        rect: Rect,
        style: &TextStyle,
        scale_factor: f32,
    ) -> Vec<DecorationStroke> {
        let spans = spans.filter(|spans| !spans.is_empty());
        let span_underlines = spans.is_some_and(|spans| {
            spans
                .iter()
                .any(|span| span.over.decoration_line == Some(DecorationLine::Underline))
        });
        if (!style.decoration.is_drawn() && !span_underlines)
            || text.is_empty()
            || rect.width <= 0.0
            || rect.height <= 0.0
        {
            return Vec::new();
        }
        self.sync_fonts();
        let cased = renderer_core::case_text(text, spans, style);
        let run_styles: Vec<TextStyle> = match cased.spans.as_deref() {
            Some(spans) => styled_runs(&cased.text, spans, style)
                .into_iter()
                .map(|(_, run_style)| run_style)
                .collect(),
            None => Vec::new(),
        };
        let buffer = make_cased_buffer(
            &mut self.font_system,
            &mut self.family_availability,
            &cased.text,
            cased.spans.as_deref(),
            rect,
            style,
        );
        let mut stretches: Vec<(f32, Vec<LayoutGlyph>)> = Vec::new();
        for run in buffer.layout_runs() {
            for stretch in run.glyphs.chunk_by(|a, b| a.metadata == b.metadata) {
                stretches.push((run.line_y, stretch.to_vec()));
            }
        }
        drop(buffer);

        let mut strokes = Vec::new();
        for (baseline, glyphs) in stretches {
            let run_style = match glyphs[0].metadata {
                0 => style,
                run => match run_styles.get(run - 1) {
                    Some(run_style) => run_style,
                    None => continue,
                },
            };
            let decoration = run_style.decoration;
            if decoration.line != DecorationLine::Underline {
                continue;
            }
            let font = self.underline_metrics(glyphs[0].font_id, glyphs[0].font_weight);
            let em = glyphs[0].font_size;
            let offset = decoration.offset.or_font(font.offset * em);
            let thickness = decoration.thickness.or_font(font.thickness * em);
            let left = glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
            let right = glyphs
                .iter()
                .map(|g| g.x + g.w)
                .fold(f32::NEG_INFINITY, f32::max);
            if right <= left {
                continue;
            }
            let device = |logical: f32| (logical * scale_factor).round() / scale_factor;
            let top = device(rect.y + baseline + offset);
            let thickness = device(thickness).max(1.0 / scale_factor);
            strokes.push(DecorationStroke {
                rect: Rect::new(rect.x + left, top, right - left, thickness),
                paint: decoration.color.unwrap_or(run_style.color),
            });
        }
        strokes
    }

    fn underline_metrics(&mut self, id: fontdb::ID, weight: fontdb::Weight) -> UnderlineMetrics {
        if let Some(known) = self.underline_metrics.get(&id) {
            return *known;
        }
        let measured = self
            .font_system
            .get_font(id, weight)
            .and_then(|font| {
                let metrics = font.as_swash().metrics(&[]);
                let em = f32::from(metrics.units_per_em);
                (em > 0.0 && metrics.stroke_size > 0.0).then(|| UnderlineMetrics {
                    offset: -metrics.underline_offset / em,
                    thickness: metrics.stroke_size / em,
                })
            })
            .unwrap_or(FALLBACK_UNDERLINE);
        self.underline_metrics.insert(id, measured);
        measured
    }
}
