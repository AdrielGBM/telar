//! Accessibility checks over what a canvas drew and what a reader is told about it.
//!
//! [`check`] reads a [`Frame`] beside the accessibility snapshot of the same moment and reports what would stop someone using the canvas with a screen reader, a keyboard, low vision or an imprecise pointer. Each finding names its [`Rule`] and carries the rule's [`Severity`]; a preview that knowingly breaks a rule says so with [`PreviewEntry::a11y_ignore`](super::PreviewEntry::a11y_ignore), and [`Report::ignoring`] drops those.

use renderer_core::LayerMask;
use std::collections::{HashMap, HashSet};
use std::fmt;
use ui_core::accessibility::{Artwork, FrameReading};
use ui_core::focus::{self, TabStop};

use crate::{
    AccessNode, Color, Direction, DrawCommand, NodeId, Paint, Rect, Role, for_each_with_matrix,
    transform_clip_rect,
};

use super::Frame;

/// The contrast body text needs against what is behind it.
pub const TEXT_CONTRAST: f32 = 4.5;
/// The contrast large text needs: 24 px, or 18.66 px at a weight of 700 and up.
pub const LARGE_TEXT_CONTRAST: f32 = 3.0;
/// The smallest a pointer target may be on either side, unless nothing else is within half of it.
pub const MIN_TARGET: f32 = 24.0;

const OPAQUE: f32 = 0.999;
const EPSILON: f32 = 0.5;

/// How much a finding matters: an [`Error`](Self::Error) keeps someone from using the canvas, a [`Warning`](Self::Warning) makes it harder or depends on where the canvas ends up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Warning,
    Error,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// What a finding is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rule {
    /// A control a reader announces by its role alone.
    UnnamedControl,
    /// A picture a reader is not told about, and not told to skip.
    UnnamedPicture,
    /// Text too close in luminance to the nearest opaque fill behind it.
    Contrast,
    /// A pointer target smaller than [`MIN_TARGET`] with something else within reach of it.
    TargetSize,
    /// A Tab stop a reader is never told is there.
    HiddenFocusable,
    /// Tab moving back against the reading order.
    FocusOrder,
    /// Text in a canvas that names no language.
    MissingLang,
}

impl Rule {
    pub const ALL: [Rule; 7] = [
        Rule::UnnamedControl,
        Rule::UnnamedPicture,
        Rule::Contrast,
        Rule::TargetSize,
        Rule::HiddenFocusable,
        Rule::FocusOrder,
        Rule::MissingLang,
    ];

    /// The rule's name in a report and on a command line.
    pub const fn id(self) -> &'static str {
        match self {
            Rule::UnnamedControl => "unnamed-control",
            Rule::UnnamedPicture => "unnamed-picture",
            Rule::Contrast => "contrast",
            Rule::TargetSize => "target-size",
            Rule::HiddenFocusable => "hidden-focusable",
            Rule::FocusOrder => "focus-order",
            Rule::MissingLang => "missing-lang",
        }
    }

    /// Target size and focus order are judged by geometry that can be right for reasons the frame does not show, and the language is often the host's to give rather than the preview's, so those three warn.
    pub const fn severity(self) -> Severity {
        match self {
            Rule::UnnamedControl
            | Rule::UnnamedPicture
            | Rule::Contrast
            | Rule::HiddenFocusable => Severity::Error,
            Rule::TargetSize | Rule::FocusOrder | Rule::MissingLang => Severity::Warning,
        }
    }

    pub fn from_id(id: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|rule| rule.id() == id)
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// One finding: the rule broken, how much it matters, what is wrong in words, and where, in the canvas's coordinates.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Violation {
    pub rule: Rule,
    pub severity: Severity,
    pub message: String,
    pub rect: Option<Rect>,
}

impl Violation {
    pub fn new(rule: Rule, message: impl Into<String>, rect: Option<Rect>) -> Self {
        Self {
            rule,
            severity: rule.severity(),
            message: message.into(),
            rect,
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:<7}  {:<16}  {}",
            self.severity.as_str(),
            self.rule.id(),
            self.message
        )
    }
}

/// Everything [`check`] found, in the order of [`Rule::ALL`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub violations: Vec<Violation>,
}

impl Report {
    /// The report without the findings of `rules`: what a preview's [`a11y_ignore`](super::PreviewEntry::a11y_ignore) asks for.
    pub fn ignoring(mut self, rules: &[Rule]) -> Self {
        self.violations
            .retain(|violation| !rules.contains(&violation.rule));
        self
    }

    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }

    /// The highest severity found, or `None` for a clean report.
    pub fn worst(&self) -> Option<Severity> {
        self.violations
            .iter()
            .map(|violation| violation.severity)
            .max()
    }

    pub fn of(&self, rule: Rule) -> impl Iterator<Item = &Violation> {
        self.violations
            .iter()
            .filter(move |violation| violation.rule == rule)
    }

    /// The findings of `severity` or worse.
    pub fn at_least(&self, severity: Severity) -> impl Iterator<Item = &Violation> {
        self.violations
            .iter()
            .filter(move |violation| violation.severity >= severity)
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, violation) in self.violations.iter().enumerate() {
            if index > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{violation}")?;
        }
        Ok(())
    }
}

/// Checks `frame` against `snapshot`, the [`accessibility::snapshot`](ui_core::accessibility::snapshot) of the same commands.
///
/// What takes focus, in what order, and what a reader skips are the surface's live state, which commands drawn on a raster target do not carry. A frame [captured](Frame::capture) from a canvas keeps them in its [`access`](Frame::access), so it can be checked whenever; one made from commands alone is read against the surface entered now, so call it with the frame's surface entered and before anything in it changes.
///
/// - **Unnamed controls and pictures:** a control whose name is empty, and a picture (a bitmap or vector art outside every control) that is neither named nor hidden from readers.
/// - **Contrast:** each run of text against the nearest opaque fill behind its centre, with the translucent fills between them composited over it, below [`TEXT_CONTRAST`], or [`LARGE_TEXT_CONTRAST`] for large text. Text over a picture or a gradient, and text in a disabled control, is not judged.
/// - **Target size:** an enabled control smaller than [`MIN_TARGET`] on a side whose 24 px circle reaches another target or another small target's circle.
/// - **Hidden focusables:** a Tab stop a reader is not told about, or one collapsed to no size.
/// - **Focus order:** a Tab stop, in the order Tab walks them, that reads before the previous one — on an earlier line and not in a later column, or earlier on the same line.
/// - **Missing language:** text that neither the canvas nor anything around it gives a language.
pub fn check(frame: &Frame, snapshot: &[AccessNode]) -> Report {
    let live;
    let (reading, tab_order) = match frame.access.as_deref() {
        Some(access) => (&access.reading, access.tab_order.as_slice()),
        None => {
            live = (FrameReading::of(&frame.commands), focus::tab_order());
            (&live.0, live.1.as_slice())
        }
    };
    let walk = Walk::of(frame);
    let mut violations = Vec::new();
    unnamed_controls(snapshot, &mut violations);
    unnamed_pictures(reading, &walk, &mut violations);
    contrast(frame, snapshot, &walk, &mut violations);
    target_sizes(snapshot, &mut violations);
    let stops = tab_stops(tab_order, snapshot, &walk);
    hidden_focusables(&stops, &mut violations);
    focus_order(frame.direction, &stops, &mut violations);
    missing_lang(frame, snapshot, &mut violations);
    Report { violations }
}

fn unnamed_controls(snapshot: &[AccessNode], out: &mut Vec<Violation>) {
    for node in snapshot {
        if node.id.is_some() && node.role.is_control() && node.name.trim().is_empty() {
            out.push(Violation::new(
                Rule::UnnamedControl,
                format!(
                    "a {} has no name, so a reader announces only its role",
                    node.role.as_str()
                ),
                Some(node.rect),
            ));
        }
    }
}

/// One finding per box: the pieces of one picture drawn in the same box are the one picture.
fn unnamed_pictures(reading: &FrameReading, walk: &Walk, out: &mut Vec<Violation>) {
    let artwork: HashMap<usize, &Artwork> = reading
        .artwork()
        .iter()
        .map(|art| (art.index, art))
        .collect();
    let mut seen = HashSet::new();
    for art in &walk.art {
        let Some(read) = artwork.get(&art.index) else {
            continue;
        };
        if read.in_control || !seen.insert(read.node) || read.skipped || read.name.is_some() {
            continue;
        }
        out.push(Violation::new(
            Rule::UnnamedPicture,
            "a picture has no name: give it one, or hide it from readers if it only decorates",
            Some(art.rect),
        ));
    }
}

fn contrast(frame: &Frame, snapshot: &[AccessNode], walk: &Walk, out: &mut Vec<Violation>) {
    let disabled: Vec<Rect> = snapshot
        .iter()
        .filter(|node| node.id.is_some() && !node.enabled)
        .map(|node| node.rect)
        .collect();
    for run in &walk.text {
        let Some(ink) = run.color else { continue };
        let (x, y) = centre(run.rect);
        if disabled.iter().any(|rect| rect.contains(x, y)) {
            continue;
        }
        let Some(behind) = walk.backdrop(run, frame.background) else {
            continue;
        };
        let ink = over(ink, behind);
        let ratio = ink.contrast_ratio(behind);
        let large = run.size >= 24.0 || (run.size >= 18.66 && run.weight >= 700);
        let needed = if large {
            LARGE_TEXT_CONTRAST
        } else {
            TEXT_CONTRAST
        };
        if ratio + 0.005 < needed {
            out.push(Violation::new(
                Rule::Contrast,
                format!(
                    "{:?} is {ratio:.2}:1 against its background; text this size needs {needed}:1",
                    run.text.trim()
                ),
                Some(run.rect),
            ));
        }
    }
}

fn target_sizes(snapshot: &[AccessNode], out: &mut Vec<Violation>) {
    let targets: Vec<&AccessNode> = snapshot
        .iter()
        .filter(|node| node.id.is_some() && node.enabled && node.role.is_control())
        .collect();
    let small =
        |rect: Rect| rect.width + EPSILON < MIN_TARGET || rect.height + EPSILON < MIN_TARGET;
    let radius = MIN_TARGET / 2.0;
    for (index, target) in targets.iter().enumerate() {
        if !small(target.rect) {
            continue;
        }
        let (x, y) = centre(target.rect);
        let crowded = targets
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .any(|(_, other)| {
                let (ox, oy) = centre(other.rect);
                distance_to_rect(x, y, other.rect) + EPSILON < radius
                    || (small(other.rect) && (x - ox).hypot(y - oy) + EPSILON < MIN_TARGET)
            });
        if crowded {
            out.push(Violation::new(
                Rule::TargetSize,
                format!(
                    "{} is {:.0}×{:.0}, smaller than {MIN_TARGET}×{MIN_TARGET} with another target within reach",
                    describe(target),
                    target.rect.width,
                    target.rect.height
                ),
                Some(target.rect),
            ));
        }
    }
}

/// A control Tab stops at, with what a reader is told about it when it is told anything.
struct Stop<'a> {
    role: Role,
    /// Where the frame places its box.
    placed: Option<Rect>,
    read: Option<&'a AccessNode>,
}

/// The stops `tab_order` names that stand for a box; a focus id that belongs to no widget has nothing to show or tell.
fn tab_stops<'a>(tab_order: &[TabStop], snapshot: &'a [AccessNode], walk: &Walk) -> Vec<Stop<'a>> {
    let read: HashMap<u64, &AccessNode> = snapshot
        .iter()
        .filter_map(|node| node.id.map(|id| (id, node)))
        .collect();
    tab_order
        .iter()
        .filter_map(|stop| {
            let node = stop.node?;
            Some(Stop {
                role: stop.role,
                placed: walk.boxes.get(&node).copied(),
                read: read.get(&stop.id).copied(),
            })
        })
        .collect()
}

fn hidden_focusables(stops: &[Stop<'_>], out: &mut Vec<Violation>) {
    for stop in stops {
        let message = match stop.read {
            None => format!(
                "a {} takes focus, but a reader is never told it is there",
                stop.role.as_str()
            ),
            Some(node) if node.rect.width < 1.0 || node.rect.height < 1.0 => format!(
                "{} takes focus, but is collapsed to no size",
                describe(node)
            ),
            Some(_) => continue,
        };
        let rect = stop.read.map(|node| node.rect).or(stop.placed);
        out.push(Violation::new(Rule::HiddenFocusable, message, rect));
    }
}

fn focus_order(direction: Direction, stops: &[Stop<'_>], out: &mut Vec<Violation>) {
    let order: Vec<&AccessNode> = stops.iter().filter_map(|stop| stop.read).collect();
    for pair in order.windows(2) {
        let [from, to] = pair else { continue };
        if reads_before(to.rect, from.rect, direction) {
            out.push(Violation::new(
                Rule::FocusOrder,
                format!(
                    "Tab moves from {} to {}, which reads before it",
                    describe(from),
                    describe(to)
                ),
                Some(to.rect),
            ));
        }
    }
}

/// Whether `next` comes before `previous` in reading order: on an earlier line without being in a later column, or earlier along the same line.
fn reads_before(next: Rect, previous: Rect, direction: Direction) -> bool {
    let rtl = direction == Direction::Rtl;
    let (next_start, next_end) = (next.x, next.x + next.width);
    let (previous_start, previous_end) = (previous.x, previous.x + previous.width);
    let later_column = if rtl {
        next_end <= previous_start + EPSILON
    } else {
        next_start + EPSILON >= previous_end
    };
    let earlier_on_line = if rtl {
        next_start + EPSILON >= previous_end
    } else {
        next_end <= previous_start + EPSILON
    };
    let above = next.y + next.height <= previous.y + EPSILON;
    let overlap = (next.y + next.height).min(previous.y + previous.height) - next.y.max(previous.y);
    let same_line = overlap >= next.height.min(previous.height) / 2.0;
    (above && !later_column) || (same_line && earlier_on_line)
}

fn missing_lang(frame: &Frame, snapshot: &[AccessNode], out: &mut Vec<Violation>) {
    if frame
        .lang
        .as_deref()
        .is_some_and(|lang| !lang.trim().is_empty())
    {
        return;
    }
    let unlabelled = snapshot
        .iter()
        .any(|node| !node.name.trim().is_empty() && node.lang.is_none());
    if unlabelled {
        out.push(Violation::new(
            Rule::MissingLang,
            "the canvas names no language, so a reader guesses how to pronounce its text",
            None,
        ));
    }
}

/// The frame as the checks read it: what is painted where, which text sits on what, the pictures, and where each box is placed.
#[derive(Default)]
struct Walk {
    fills: Vec<Fill>,
    text: Vec<Run>,
    art: Vec<Art>,
    boxes: HashMap<NodeId, Rect>,
}

enum Fill {
    Solid {
        rect: Rect,
        color: Color,
    },
    /// Paint whose colour under a point the frame does not say: a picture, a gradient, a filled path.
    Unknown {
        rect: Rect,
    },
}

struct Run {
    text: String,
    rect: Rect,
    /// `None` for ink that is not one colour.
    color: Option<Color>,
    size: f32,
    weight: u16,
    /// How many fills were painted before it.
    over: usize,
}

struct Art {
    index: usize,
    rect: Rect,
}

#[derive(Clone, Copy)]
struct Layer {
    opacity: f32,
    hidden: bool,
}

impl Walk {
    fn of(frame: &Frame) -> Self {
        let mut walk = Walk::default();
        let unbounded = Rect::new(
            -f32::MAX / 4.0,
            -f32::MAX / 4.0,
            f32::MAX / 2.0,
            f32::MAX / 2.0,
        );
        let mut clips = vec![unbounded];
        let mut layers = vec![Layer {
            opacity: 1.0,
            hidden: false,
        }];
        let mut at = 0usize;
        for_each_with_matrix(&frame.commands, |command, matrix| {
            let index = at;
            at += 1;
            let clip = *clips.last().unwrap_or(&unbounded);
            let layer = *layers.last().unwrap_or(&Layer {
                opacity: 1.0,
                hidden: false,
            });
            let placed = |rect: Rect| {
                transform_clip_rect(matrix, rect)
                    .intersect(clip)
                    .filter(|_| !layer.hidden && layer.opacity > 0.0)
            };
            match command {
                DrawCommand::PushElement { element } => {
                    walk.boxes
                        .entry(NodeId::from(element.id.0))
                        .or_insert(element.rect);
                }
                DrawCommand::PushClip { rect, .. } => {
                    let placed = transform_clip_rect(matrix, *rect)
                        .intersect(clip)
                        .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0));
                    clips.push(placed);
                }
                DrawCommand::PopClip => {
                    clips.pop();
                }
                DrawCommand::PushLayer { opacity, mask, .. } => layers.push(Layer {
                    opacity: layer.opacity * opacity,
                    hidden: layer.hidden || *mask == LayerMask::Source,
                }),
                DrawCommand::PopLayer => {
                    layers.pop();
                }
                DrawCommand::Rect { rect, style } => {
                    let (Some(fill), Some(rect)) = (&style.fill, placed(*rect)) else {
                        return;
                    };
                    walk.fills.push(match fill {
                        Paint::Solid(color) if color.a * layer.opacity > 0.0 => Fill::Solid {
                            rect,
                            color: color.with_alpha(color.a * layer.opacity),
                        },
                        Paint::Solid(_) => return,
                        Paint::Gradient(_) => Fill::Unknown { rect },
                    });
                }
                DrawCommand::Image { rect, .. } => {
                    if let Some(placed) = placed(*rect) {
                        walk.fills.push(Fill::Unknown { rect: placed });
                        walk.art.push(Art {
                            index,
                            rect: placed,
                        });
                    }
                }
                DrawCommand::Path { data, style } => {
                    let inked = |paint: Option<&Paint>| {
                        paint.is_some_and(|paint| match paint {
                            Paint::Solid(color) => color.a > 0.0,
                            Paint::Gradient(_) => true,
                        })
                    };
                    let filled = inked(style.fill.as_ref());
                    let stroked = inked(style.stroke.as_ref().map(|stroke| &stroke.paint));
                    let Some(placed) = data.bounds().and_then(placed) else {
                        return;
                    };
                    if filled {
                        walk.fills.push(Fill::Unknown { rect: placed });
                    }
                    if filled || stroked {
                        walk.art.push(Art {
                            index,
                            rect: placed,
                        });
                    }
                }
                DrawCommand::Text {
                    text, rect, style, ..
                } => {
                    if text.trim().is_empty() {
                        return;
                    }
                    let Some(rect) = placed(*rect) else { return };
                    let color = match style.color {
                        Paint::Solid(color) if color.a * layer.opacity > 0.0 => {
                            Some(color.with_alpha(color.a * layer.opacity))
                        }
                        Paint::Solid(_) => return,
                        Paint::Gradient(_) => None,
                    };
                    walk.text.push(Run {
                        text: text.to_string(),
                        rect,
                        color,
                        size: style.font_size * scale_of(matrix),
                        weight: style.font_weight,
                        over: walk.fills.len(),
                    });
                }
                _ => {}
            }
        });
        walk
    }

    /// The colour behind `run`'s centre: the nearest opaque fill under it, or `background` under everything, with the translucent fills between composited over it. `None` where something whose colour is unknown is in the way, or nothing is.
    fn backdrop(&self, run: &Run, background: Option<Color>) -> Option<Color> {
        let (x, y) = centre(run.rect);
        let mut between = Vec::new();
        let mut base = None;
        for fill in self.fills[..run.over].iter().rev() {
            match fill {
                Fill::Unknown { rect } if rect.contains(x, y) => return None,
                Fill::Solid { rect, color } if rect.contains(x, y) => {
                    if color.a >= OPAQUE {
                        base = Some(*color);
                        break;
                    }
                    between.push(*color);
                }
                _ => {}
            }
        }
        let base = base.or(background.map(|color| color.with_alpha(1.0)))?;
        Some(
            between
                .into_iter()
                .rev()
                .fold(base, |under, top| over(top, under)),
        )
    }
}

/// `top` painted over the opaque `under`.
fn over(top: Color, under: Color) -> Color {
    let a = top.a.clamp(0.0, 1.0);
    Color::rgba(
        top.r * a + under.r * (1.0 - a),
        top.g * a + under.g * (1.0 - a),
        top.b * a + under.b * (1.0 - a),
        1.0,
    )
}

fn scale_of(matrix: [f32; 6]) -> f32 {
    let [a, b, c, d, ..] = matrix;
    (a * d - b * c).abs().sqrt()
}

fn centre(rect: Rect) -> (f32, f32) {
    (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

fn distance_to_rect(x: f32, y: f32, rect: Rect) -> f32 {
    let dx = (rect.x - x).max(0.0).max(x - (rect.x + rect.width));
    let dy = (rect.y - y).max(0.0).max(y - (rect.y + rect.height));
    dx.hypot(dy)
}

fn describe(node: &AccessNode) -> String {
    match node.name.trim() {
        "" => format!("an unnamed {}", node.role.as_str()),
        name => format!("{} {name:?}", node.role.as_str()),
    }
}

#[cfg(test)]
#[path = "a11y_test.rs"]
mod tests;
