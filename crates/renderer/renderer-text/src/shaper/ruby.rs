//! Readings set above their bases: each fitted to its base before the paragraph is shaped, and placed, with room made for it, once the paragraph is laid into lines.

use cosmic_text::{Buffer, FontSystem, LayoutGlyph, LayoutRun};
use geometry_core::Rect;
use renderer_core::{TextAlign, TextCase, TextDecoration, TextStyle, TextWrap, WORD_JOINER};
use std::sync::Arc;

use super::{build_buffer, to_cosmic_color};

/// How far above its baseline a face's ideographic em box reaches, as a fraction of its size: what a reading sits on, so it sits as close over a kanji as the kanji's own box allows.
const EM_ASCENT: f32 = 0.88;
/// How far below its baseline a face's ideographic em box reaches, as a fraction of its size.
const EM_DESCENT: f32 = 0.12;

/// One reading of a paragraph, fitted to its base and shaped on its own line.
pub(super) struct Reading {
    /// The metadata the base's glyphs carry: its run's place in the paragraph, counted from one.
    run: usize,
    /// Its glyphs, set from the reading's left edge on a baseline at zero, already tightened.
    glyphs: Vec<LayoutGlyph>,
    width: f32,
    /// The size it is set in.
    size: f32,
    /// The height of the band it is set in above its base: its size before it was fitted, so every reading of a line asks the same room whatever it shrank to.
    band: f32,
    /// What each glyph of the base was widened by to hold a reading that could not be made to fit it.
    spacing: f32,
}

/// A paragraph shaped into lines, with the readings set above its bases.
pub(super) struct Laid {
    pub(super) buffer: Buffer,
    pub(super) readings: Vec<Reading>,
}

/// How a visual line moves once the readings above it have room: `grow` is the room its line box gains above its own glyphs, `drop` how far down its glyphs move, that room and every line's above it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Lift {
    pub(super) drop: f32,
    pub(super) grow: f32,
}

/// Where a base lies on a line: the left and right edges of its glyphs, and the size it is set in.
struct Base {
    left: f32,
    right: f32,
    size: f32,
}

/// The reading over the base whose glyphs carry `metadata`.
fn reading_of(readings: &[Reading], metadata: usize) -> Option<&Reading> {
    readings.iter().find(|reading| reading.run == metadata)
}

/// What each glyph of the base whose glyphs carry `metadata` is widened by, or nothing for a run that is no base.
pub(super) fn spacing_of(readings: &[Reading], metadata: usize) -> f32 {
    reading_of(readings, metadata).map_or(0.0, |reading| reading.spacing)
}

fn base_on(run: &LayoutRun<'_>, metadata: usize) -> Option<Base> {
    let mut glyphs = run.glyphs.iter().filter(|glyph| glyph.metadata == metadata);
    let first = glyphs.next()?;
    let mut base = Base {
        left: first.x,
        right: first.x + first.w,
        size: first.font_size,
    };
    for glyph in glyphs {
        base.left = base.left.min(glyph.x);
        base.right = base.right.max(glyph.x + glyph.w);
    }
    Some(base)
}

impl Laid {
    pub(super) fn reading_of(&self, metadata: usize) -> Option<&Reading> {
        reading_of(&self.readings, metadata)
    }

    /// The lift of each visual line, in order, with the baseline drawn `shift` below where cosmic-text puts it. A line grows by what of the band its readings sit in reaches above its line box, in whole pixels so the lines below stay on the pixel grid; a paragraph without readings, or a line whose leading already holds them, does not move.
    pub(super) fn lifts(&self, shift: f32) -> Vec<Lift> {
        let mut drop = 0.0;
        self.buffer
            .layout_runs()
            .map(|run| {
                let reach = self
                    .readings
                    .iter()
                    .filter_map(|reading| {
                        let base = base_on(&run, reading.run)?;
                        let band_top = run.line_y + shift - EM_ASCENT * base.size - reading.band;
                        Some(run.line_top - band_top)
                    })
                    .fold(0.0_f32, f32::max);
                let grow = reach.max(0.0).ceil();
                drop += grow;
                Lift { drop, grow }
            })
            .collect()
    }

    /// Every glyph to draw, each with the baseline it sits on: the paragraph's own, each base's spread across the room its reading needed, and each reading centred over the first line its base is on.
    pub(super) fn placed_glyphs(&self, shift: f32) -> Vec<(LayoutGlyph, f32)> {
        let lifts = self.lifts(shift);
        let mut placed = Vec::new();
        let mut set = vec![false; self.readings.len()];
        for (line, run) in self.buffer.layout_runs().enumerate() {
            let baseline = run.line_y + shift + lifts.get(line).map_or(0.0, |lift| lift.drop);
            for glyph in run.glyphs {
                let mut glyph = glyph.clone();
                if let Some(reading) = self.reading_of(glyph.metadata) {
                    glyph.x += reading.spacing / 2.0;
                }
                placed.push((glyph, baseline));
            }
            for (reading, set) in self.readings.iter().zip(set.iter_mut()) {
                if *set {
                    continue;
                }
                let Some(base) = base_on(&run, reading.run) else {
                    continue;
                };
                *set = true;
                let left = (base.left + base.right - reading.width) / 2.0;
                let reading_baseline = baseline - EM_ASCENT * base.size - EM_DESCENT * reading.size;
                placed.extend(reading.glyphs.iter().map(|glyph| {
                    let mut glyph = glyph.clone();
                    glyph.x += left;
                    (glyph, reading_baseline)
                }));
            }
        }
        placed
    }
}

/// The style a reading over a run in `run_style` is set in at `size`: the run's face and ink on one line, with nothing the run spreads, decorates or cases its own letters with.
fn reading_style(run_style: &TextStyle, size: f32) -> TextStyle {
    TextStyle {
        font_size: size,
        letter_spacing: 0.0,
        text_wrap: TextWrap::NoWrap,
        clamp: None,
        text_align: TextAlign::Start,
        line_height: renderer_core::LineHeight::Natural,
        text_case: TextCase::AsWritten,
        decoration: TextDecoration::default(),
        ..run_style.clone()
    }
}

/// `text` shaped on one line in `style`: its glyphs, and how far it advances.
fn one_line(
    font_system: &mut FontSystem,
    family_availability: &mut rustc_hash::FxHashMap<Arc<str>, bool>,
    text: &str,
    style: &TextStyle,
) -> (Vec<LayoutGlyph>, f32) {
    let style = style.clone().with_text_wrap(TextWrap::NoWrap);
    let rect = Rect::new(0.0, 0.0, f32::INFINITY, 100_000.0);
    let buffer = build_buffer(
        font_system,
        family_availability,
        text,
        None,
        rect,
        &style,
        &[],
    );
    let mut glyphs = Vec::new();
    let mut width: f32 = 0.0;
    for run in buffer.layout_runs() {
        glyphs.extend(run.glyphs.iter().cloned());
        width = width.max(run.line_w);
    }
    (glyphs, width)
}

/// Fits `reading` over `base`, a run of the paragraph in `run_style` whose glyphs carry `run` as their metadata (see [`renderer_core::RubyStyle::fit`]).
pub(super) fn fit_reading(
    font_system: &mut FontSystem,
    family_availability: &mut rustc_hash::FxHashMap<Arc<str>, bool>,
    run: usize,
    base: &str,
    run_style: &TextStyle,
    reading: &str,
) -> Reading {
    let (base_glyphs, base_width) = one_line(font_system, family_availability, base, run_style);
    let spread = base_glyphs
        .iter()
        .filter(|glyph| !base[glyph.start..glyph.end].starts_with(WORD_JOINER))
        .count()
        .max(1);
    let nominal = run_style.ruby.nominal(run_style.font_size);
    let (mut glyphs, width) = one_line(
        font_system,
        family_availability,
        reading,
        &reading_style(run_style, nominal),
    );
    let fit = run_style
        .ruby
        .fit(run_style.font_size, base_width, width, glyphs.len());
    if fit.size != nominal {
        glyphs = one_line(
            font_system,
            family_availability,
            reading,
            &reading_style(run_style, fit.size),
        )
        .0;
    }
    let ink = to_cosmic_color(run_style.color.solid_color());
    for (at, glyph) in glyphs.iter_mut().enumerate() {
        glyph.x -= at as f32 * fit.tightening;
        glyph.color_opt = Some(ink);
    }
    Reading {
        run,
        glyphs,
        width: fit.width,
        size: fit.size,
        band: nominal,
        spacing: fit.widening / spread as f32,
    }
}

#[cfg(test)]
#[path = "ruby.test.rs"]
mod tests;
