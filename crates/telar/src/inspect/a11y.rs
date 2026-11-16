//! Accessibility checks over what a canvas drew and what a reader is told about it.
//!
//! [`check`] reads a [`Frame`] beside the accessibility snapshot of the same moment and reports what would stop someone using the canvas with a screen reader, a keyboard, low vision or an imprecise pointer; [`check_focus`] reads the frames the canvas draws as the keyboard reaches each of its stops. Each finding names its [`Rule`] and carries the rule's [`Severity`]; a preview that knowingly breaks a rule says so with its `a11y_ignore`, and [`Report::ignoring`] drops those.

use renderer_core::{FillRule, LayerMask, PathData};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use ui_core::accessibility::{Artwork, FrameReading};
use ui_core::focus::{self, TabStop};

use crate::{
    AccessNode, Color, ContrastLevel, ContrastUse, Direction, DrawCommand, NodeId, Paint, Point,
    Rect, Role, Transform, for_each_with_matrix, transform_clip_rect,
};

use super::Frame;

/// The smallest a pointer target may be on either side, unless nothing else is within half of it.
pub const MIN_TARGET: f32 = 24.0;

const OPAQUE: f32 = 0.999;
const EPSILON: f32 = 0.5;
/// How far outside a ring the colour around it is read, so the ring's own antialiased edge is not mistaken for it.
const AROUND: f32 = 1.0;

/// How much a finding matters, what it is about and the finding itself: shared with the platform layer, so a finding made where an application's registries live reaches an overlay drawn somewhere else.
pub use platform_core::{
    AccessRule as Rule, AccessSeverity as Severity, AccessViolation as Violation,
};

/// Everything [`check`] found, in the order of [`Rule::ALL`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    /// Every finding, in the order of [`Rule::ALL`].
    pub violations: Vec<Violation>,
}

impl Report {
    /// The report without the findings of `rules`: what a preview's `a11y_ignore` asks for.
    pub fn ignoring(mut self, rules: &[Rule]) -> Self {
        self.violations
            .retain(|violation| !rules.contains(&violation.rule));
        self
    }

    /// This report and `other` as one, still in the order of [`Rule::ALL`].
    pub fn and(mut self, other: Report) -> Self {
        self.violations.extend(other.violations);
        self.violations
            .sort_by_key(|violation| Rule::ALL.iter().position(|rule| *rule == violation.rule));
        self
    }

    /// Whether nothing was found.
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

    /// The violations of `rule`.
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

/// Checks `frame` against `snapshot`, the [`accessibility::surface_snapshot`](ui_core::accessibility::surface_snapshot) of the same commands. A surface nested in the frame is left to [`audit`](super::audit), which checks it in its own world.
///
/// What takes focus, in what order, and what a reader skips are the surface's live state, which commands drawn on a raster target do not carry. A frame [captured](Frame::capture) from a canvas keeps them in its [`access`](Frame::access), so it can be checked whenever; one made from commands alone is read against the surface entered now, so call it with the frame's surface entered and before anything in it changes.
///
/// - **Unnamed controls and pictures:** a control whose name is empty, and a picture (a bitmap or vector art outside every control) that is neither named nor hidden from readers.
/// - **Unnamed widgets:** a list box, a tree, a grid, a tab list, a radio group, a dialog or a progress bar whose name is empty.
/// - **Symbol-only names:** a control activated by pressing it whose name has no letter or number in it, such as "×" or "⋯".
/// - **Contrast:** each run of text against the nearest opaque fill behind its centre, a filled path included where its shape covers that point, with the translucent fills between them composited over it, below [`BODY_TEXT_CONTRAST`](geometry_core::BODY_TEXT_CONTRAST), or [`LARGE_TEXT_CONTRAST`](geometry_core::LARGE_TEXT_CONTRAST) for large text. Text over a picture or a gradient, and text in a disabled control, is not judged.
/// - **Boundary contrast:** an enabled field, box to tick, switch, slider or select none of whose fills and edges stands the [`ContrastUse::ControlBoundary`] minimum from what it is drawn on, so nothing shows where it is. A control that draws nothing inside its own box is judged by the innermost box drawn around it, as a field by its frame. One drawn over a picture or a gradient is not judged.
/// - **Target size:** an enabled control smaller than [`MIN_TARGET`] on a side whose 24 px circle reaches another target or another small target's circle.
/// - **Hidden focusables:** a Tab stop a reader is not told about, or one collapsed to no size.
/// - **Focus order:** a Tab stop, in the order Tab walks them, that reads before the previous one — on an earlier line and not in a later column, or earlier on the same line.
/// - **Missing language:** text that neither the canvas nor anything around it gives a language.
pub fn check(frame: &Frame, snapshot: &[AccessNode]) -> Report {
    check_over(frame, snapshot, &|_, _| frame.background)
}

/// [`check`] for a surface drawn inside another's frame: `behind` is the colour showing under a point of the surface, in its own coordinates, wherever the surface paints nothing opaque itself.
pub(crate) fn check_over(
    frame: &Frame,
    snapshot: &[AccessNode],
    behind: &dyn Fn(f32, f32) -> Option<Color>,
) -> Report {
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
    unnamed_widgets(snapshot, &mut violations);
    symbol_only_names(snapshot, &mut violations);
    unnamed_pictures(reading, &walk, &mut violations);
    contrast(snapshot, &walk, behind, &mut violations);
    boundary_contrast(snapshot, &walk, behind, &mut violations);
    target_sizes(snapshot, &mut violations);
    let stops = tab_stops(tab_order, snapshot, &walk);
    hidden_focusables(&stops, &mut violations);
    focus_order(frame.direction, &stops, &mut violations);
    missing_lang(frame, snapshot, &mut violations);
    Report { violations }
}

/// A Tab stop as the keyboard reaches it: what a reader is told about it then, and the frame the canvas draws while it holds focus.
#[derive(Clone, Debug)]
pub struct FocusedStop {
    /// The stop as a reader is told about it while it holds focus.
    pub node: AccessNode,
    /// The canvas while the stop holds focus.
    pub frame: Frame,
}

/// Checks that each of `focused` shows where the keyboard is: `rest` is the same canvas with nothing focused.
///
/// What the keyboard reaching a stop changes inside its box is its focus indicator. A ring is judged as the keyboard's ring is drawn, in two bands so that one of them always shows: the better band has to stand the [`ContrastUse::ControlBoundary`] minimum from the stop's fill and from what is around the ring. An indicator drawn as a change of fill alone has to stand as far from what was there before. A stop whose box looks the same focused or not shows nothing at all.
///
/// A stop whose cursor rests on one of its items, as a menu's or a tree's does, shows the keyboard on that item: what is judged is what focus changes there.
pub fn check_focus(rest: &Frame, focused: &[FocusedStop]) -> Report {
    let before = Walk::of(rest);
    let violations = focused
        .iter()
        .filter_map(|stop| focus_indicator(&before, rest.background, stop))
        .collect();
    Report { violations }
}

fn focus_indicator(
    before: &Walk,
    background: Option<Color>,
    stop: &FocusedStop,
) -> Option<Violation> {
    let walk = Walk::of(&stop.frame);
    let background = stop.frame.background.or(background);
    let pointed = stop.node.active_descendant.and_then(|item| {
        stop.frame
            .access
            .as_deref()?
            .snapshot
            .iter()
            .find(|node| node.id == Some(item))
    });
    let area = pointed.map_or(stop.node.rect, |item| item.rect);
    let changed: Vec<&Shape> = walk
        .shapes
        .iter()
        .filter(|shape| shape.rect.overlaps(area))
        .filter(|shape| !before.shapes.iter().any(|old| old.looks_like(shape)))
        .collect();
    let needed = ContrastUse::ControlBoundary.minimum(ContrastLevel::Standard);
    let violation =
        |message: String| Some(Violation::new(Rule::FocusIndicator, message, Some(area)));
    if changed.is_empty() {
        return violation(match pointed {
            Some(item) => format!(
                "nothing marks {}, which the cursor of {} rests on",
                describe(item),
                describe(&stop.node)
            ),
            None => format!(
                "{} looks the same with the keyboard on it: nothing shows where focus is",
                describe(&stop.node)
            ),
        });
    }
    let bands: Vec<(&Shape, Color)> = changed
        .iter()
        .filter_map(|shape| match shape.edge_ink() {
            Some(Ink::Solid(color)) => Some((*shape, color)),
            _ => None,
        })
        .collect();
    if bands.is_empty() {
        let best = changed
            .iter()
            .filter(|shape| matches!(shape.fill, Some(Ink::Solid(_))))
            .filter_map(|shape| {
                let (x, y) = centre(shape.rect);
                let now = walk.backdrop(x, y, shape.upto, background)?;
                let was = before.backdrop(x, y, before.fills.len(), background)?;
                Some(now.contrast_ratio(was))
            })
            .fold(None, |best: Option<f32>, ratio| {
                Some(best.map_or(ratio, |best| best.max(ratio)))
            })?;
        return (best + 0.005 < needed).then(|| {
            Violation::new(
                Rule::FocusIndicator,
                format!(
                    "{} shows focus only by a change of fill {best:.2}:1 from what was there; it needs {needed}:1",
                    describe(&stop.node)
                ),
                Some(area),
            )
        });
    }
    let ring = bands
        .iter()
        .map(|(shape, _)| shape.rect)
        .reduce(Rect::union)
        .unwrap_or(area);
    let under = bands
        .iter()
        .map(|(shape, _)| shape.under)
        .min()
        .unwrap_or(0);
    let upto = bands.iter().map(|(shape, _)| shape.upto).max().unwrap_or(0);
    let (x, y) = centre(area);
    let fill = walk.backdrop(x, y, upto, background);
    let (rx, ry) = centre(ring);
    let around = [
        (ring.x - AROUND, ry),
        (ring.x + ring.width + AROUND, ry),
        (rx, ring.y - AROUND),
        (rx, ring.y + ring.height + AROUND),
    ]
    .map(|(x, y)| walk.backdrop(x, y, under, background));
    let better_band = |against: Color| {
        bands
            .iter()
            .map(|(_, band)| band.over(against).contrast_ratio(against))
            .fold(0.0f32, f32::max)
    };
    let short = std::iter::once(("its fill", fill))
        .chain(around.map(|colour| ("what is around it", colour)))
        .filter_map(|(what, against)| against.map(|against| (what, better_band(against))))
        .filter(|(_, ratio)| ratio + 0.005 < needed)
        .min_by(|(_, a), (_, b)| a.total_cmp(b));
    let (what, ratio) = short?;
    violation(format!(
        "the ring {} wears with the keyboard on it is {ratio:.2}:1 against {what} at best; it needs {needed}:1",
        describe(&stop.node)
    ))
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

fn unnamed_widgets(snapshot: &[AccessNode], out: &mut Vec<Violation>) {
    for node in snapshot {
        if needs_name(&node.role) && node.name.trim().is_empty() {
            out.push(Violation::new(
                Rule::UnnamedWidget,
                format!(
                    "a {} has no name, so a reader cannot tell it from another: name it with `a11y_label`",
                    node.role.as_str()
                ),
                Some(node.rect),
            ));
        }
    }
}

/// What a reader moves into or is told about as a whole, and tells from another of its kind only by its name.
fn needs_name(role: &Role) -> bool {
    matches!(
        role,
        Role::ListBox
            | Role::Tree
            | Role::Grid
            | Role::TabList
            | Role::RadioGroup
            | Role::Dialog
            | Role::ProgressBar
    )
}

fn symbol_only_names(snapshot: &[AccessNode], out: &mut Vec<Violation>) {
    for node in snapshot {
        let name = node.name.trim();
        if node.id.is_some()
            && activates(&node.role)
            && !name.is_empty()
            && !has_word_character(name)
        {
            out.push(Violation::new(
                Rule::SymbolOnlyName,
                format!(
                    "{} is named only by symbols, which a reader spells out or skips: give it a name in words",
                    describe(node)
                ),
                Some(node.rect),
            ));
        }
    }
}

fn activates(role: &Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::Link(_)
            | Role::CheckBox
            | Role::Radio
            | Role::Switch
            | Role::Tab
            | Role::MenuItem
            | Role::MenuItemCheckBox
            | Role::MenuItemRadio
            | Role::Option
            | Role::Disclosure
            | Role::TreeItem
    )
}

fn has_word_character(name: &str) -> bool {
    name.chars().any(char::is_alphanumeric)
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

fn contrast(
    snapshot: &[AccessNode],
    walk: &Walk,
    behind: &dyn Fn(f32, f32) -> Option<Color>,
    out: &mut Vec<Violation>,
) {
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
        let Some(backdrop) = walk.backdrop(x, y, run.over, behind(x, y)) else {
            continue;
        };
        let ink = ink.over(backdrop);
        let ratio = ink.contrast_ratio(backdrop);
        let large = run.size >= 24.0 || (run.size >= 18.66 && run.weight >= 700);
        let needed = if large {
            geometry_core::LARGE_TEXT_CONTRAST
        } else {
            geometry_core::BODY_TEXT_CONTRAST
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

/// A control a person finds by its edges rather than by words it draws: a button or a link says what it is in its label, a field drawn with no edge is an empty stretch of page.
fn found_by_edges(role: &Role) -> bool {
    matches!(
        role,
        Role::TextInput
            | Role::MultilineTextInput
            | Role::ComboBox
            | Role::SpinButton
            | Role::CheckBox
            | Role::Radio
            | Role::Switch
            | Role::Slider
    )
}

fn boundary_contrast(
    snapshot: &[AccessNode],
    walk: &Walk,
    behind: &dyn Fn(f32, f32) -> Option<Color>,
    out: &mut Vec<Violation>,
) {
    let needed = ContrastUse::ControlBoundary.minimum(ContrastLevel::Standard);
    for node in snapshot {
        if node.id.is_none() || !node.enabled || !found_by_edges(&node.role) {
            continue;
        }
        let mut best = 0.0f32;
        let mut unknown = false;
        for shape in edges_of(walk, node.rect) {
            let (x, y) = centre(shape.rect);
            let Some(under) = walk.backdrop(x, y, shape.under, behind(x, y)) else {
                unknown |= walk.obscured(x, y, shape.under);
                continue;
            };
            for ink in [shape.fill, shape.edge_ink()].into_iter().flatten() {
                match ink {
                    Ink::Solid(color) => best = best.max(color.over(under).contrast_ratio(under)),
                    Ink::Unknown => unknown = true,
                }
            }
        }
        if best + 0.005 < needed && !unknown {
            out.push(Violation::new(
                Rule::BoundaryContrast,
                format!(
                    "{} stands {best:.2}:1 from what it is drawn on at best, so nothing shows where it is; its edge or fill needs {needed}:1",
                    describe(node)
                ),
                Some(node.rect),
            ));
        }
    }
}

/// What draws the edges of a control at `rect`: the shapes inside it, or, where it draws none of its own, the innermost shape around it, as a field's frame is drawn round the line it types into.
fn edges_of(walk: &Walk, rect: Rect) -> Vec<&Shape> {
    let inside: Vec<&Shape> = walk
        .shapes
        .iter()
        .filter(|shape| within(shape.rect, rect))
        .collect();
    if !inside.is_empty() {
        return inside;
    }
    walk.shapes
        .iter()
        .filter(|shape| within(rect, shape.rect))
        .min_by(|a, b| area(a.rect).total_cmp(&area(b.rect)))
        .into_iter()
        .collect()
}

fn area(rect: Rect) -> f32 {
    rect.width * rect.height
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
    let read: HashMap<platform_core::AccessId, &AccessNode> = snapshot
        .iter()
        .filter_map(|node| node.id.map(|id| (id, node)))
        .collect();
    tab_order
        .iter()
        .filter_map(|stop| {
            let node = stop.node?;
            Some(Stop {
                role: stop.role.clone(),
                placed: walk.boxes.get(&node).copied(),
                read: read.get(&platform_core::AccessId::own(stop.id)).copied(),
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
    if frame.lang.is_some() {
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

/// What a frame paints, read for the colour it shows under a surface drawn inside it.
pub(crate) struct Painted(Walk);

impl Painted {
    pub(crate) fn of(frame: &Frame) -> Self {
        Self(Walk::of(frame))
    }

    /// The colour the frame shows at (`x`, `y`), with `background` under everything it paints.
    pub(crate) fn at(&self, x: f32, y: f32, background: Option<Color>) -> Option<Color> {
        self.0.backdrop(x, y, self.0.fills.len(), background)
    }
}

/// The frame as the checks read it: what is painted where, the shapes that paint it, which text sits on what, the pictures, and where each box is placed.
#[derive(Default)]
struct Walk {
    fills: Vec<Fill>,
    shapes: Vec<Shape>,
    text: Vec<Run>,
    art: Vec<Art>,
    boxes: HashMap<NodeId, Rect>,
}

enum Fill {
    Solid {
        rect: Rect,
        color: Color,
    },
    /// A path filled in one colour, which paints only the points its shape covers within `rect`, its placed bounds.
    Path {
        rect: Rect,
        color: Color,
        data: Arc<PathData>,
        rule: FillRule,
        /// From the frame's coordinates back to the path's own.
        inverse: Transform,
    },
    /// Paint whose colour under a point the frame does not say: a picture, a gradient.
    Unknown {
        rect: Rect,
    },
}

impl Fill {
    /// What the fill paints at (`x`, `y`), or `None` where it paints nothing.
    fn at(&self, x: f32, y: f32) -> Option<Ink> {
        match self {
            Fill::Solid { rect, color } => rect.contains(x, y).then_some(Ink::Solid(*color)),
            Fill::Path {
                rect,
                color,
                data,
                rule,
                inverse,
            } => (rect.contains(x, y) && data.contains(inverse.apply(Point::new(x, y)), *rule))
                .then_some(Ink::Solid(*color)),
            Fill::Unknown { rect } => rect.contains(x, y).then_some(Ink::Unknown),
        }
    }
}

/// The paint of a shape's fill or edge: one colour, or one that varies across it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ink {
    Solid(Color),
    Unknown,
}

impl Ink {
    fn of(paint: &Paint, opacity: f32) -> Option<Self> {
        match paint {
            Paint::Solid(color) if color.a * opacity > 0.0 => {
                Some(Ink::Solid(color.with_alpha(color.a * opacity)))
            }
            Paint::Solid(_) => None,
            Paint::Gradient(_) => Some(Ink::Unknown),
        }
    }
}

/// A box or a path as painted: what tells a control from what it is drawn on, and what a focus ring is drawn with.
struct Shape {
    rect: Rect,
    fill: Option<Ink>,
    edge: Option<Edge>,
    /// How many fills were painted before it.
    under: usize,
    /// How many fills were painted once its own was.
    upto: usize,
}

/// A box's border or a path's stroke: its paint, and how thick it is on each side.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Edge {
    ink: Ink,
    widths: [f32; 4],
}

impl Shape {
    fn looks_like(&self, other: &Shape) -> bool {
        self.rect == other.rect && self.fill == other.fill && self.edge == other.edge
    }

    fn edge_ink(&self) -> Option<Ink> {
        self.edge.map(|edge| edge.ink)
    }
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
        let mut nested = 0usize;
        for_each_with_matrix(&frame.commands, |command, matrix| {
            let index = at;
            at += 1;
            // A surface nested in the frame is checked on its own, in its own world: see `super::audit`.
            match command {
                DrawCommand::PushSurface { .. } => {
                    nested += 1;
                    return;
                }
                DrawCommand::PopSurface => {
                    nested = nested.saturating_sub(1);
                    return;
                }
                _ if nested > 0 => return,
                _ => {}
            }
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
                    let Some(rect) = placed(*rect) else { return };
                    let fill = style
                        .fill
                        .as_ref()
                        .and_then(|fill| Ink::of(fill, layer.opacity));
                    let edge = style
                        .border
                        .filter(|border| border.widths.iter().any(|width| *width > 0.0))
                        .and_then(|border| {
                            Some(Edge {
                                ink: Ink::of(&border.paint, layer.opacity)?,
                                widths: border.widths,
                            })
                        });
                    let under = walk.fills.len();
                    match fill {
                        Some(Ink::Solid(color)) => walk.fills.push(Fill::Solid { rect, color }),
                        Some(Ink::Unknown) => walk.fills.push(Fill::Unknown { rect }),
                        None => {}
                    }
                    walk.push_shape(rect, fill, edge, under);
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
                    let fill = style
                        .fill
                        .as_ref()
                        .and_then(|fill| Ink::of(fill, layer.opacity));
                    let edge = style.stroke.as_ref().and_then(|stroke| {
                        Some(Edge {
                            ink: Ink::of(&stroke.paint, layer.opacity)?,
                            widths: [stroke.width; 4],
                        })
                    });
                    let Some(rect) = data.bounds().and_then(placed) else {
                        return;
                    };
                    let under = walk.fills.len();
                    match (fill, matrix.invert()) {
                        (Some(Ink::Solid(color)), Some(inverse)) => walk.fills.push(Fill::Path {
                            rect,
                            color,
                            data: Arc::clone(data),
                            rule: style.fill_rule,
                            inverse,
                        }),
                        (Some(_), _) => walk.fills.push(Fill::Unknown { rect }),
                        (None, _) => {}
                    }
                    if fill.is_some() || edge.is_some() {
                        walk.art.push(Art { index, rect });
                    }
                    walk.push_shape(rect, fill, edge, under);
                }
                DrawCommand::Text {
                    text, rect, style, ..
                } => {
                    if text.trim().is_empty() {
                        return;
                    }
                    let Some(rect) = placed(*rect) else { return };
                    let color = match Ink::of(&style.color, layer.opacity) {
                        Some(Ink::Solid(color)) => Some(color),
                        Some(Ink::Unknown) => None,
                        None => return,
                    };
                    walk.text.push(Run {
                        text: text.to_string(),
                        rect,
                        color,
                        size: style.font_size * matrix.scale(),
                        weight: style.font_weight,
                        over: walk.fills.len(),
                    });
                }
                _ => {}
            }
        });
        walk
    }

    fn push_shape(&mut self, rect: Rect, fill: Option<Ink>, edge: Option<Edge>, under: usize) {
        if fill.is_some() || edge.is_some() {
            self.shapes.push(Shape {
                rect,
                fill,
                edge,
                under,
                upto: self.fills.len(),
            });
        }
    }

    /// Whether something whose colour is unknown is painted at (`x`, `y`) under the first `under` fills, rather than nothing at all.
    fn obscured(&self, x: f32, y: f32, under: usize) -> bool {
        self.fills[..under]
            .iter()
            .any(|fill| fill.at(x, y) == Some(Ink::Unknown))
    }

    /// The colour at (`x`, `y`) under the first `under` fills: the nearest opaque fill there, or `background` under everything, with the translucent fills between composited over it. `None` where something whose colour is unknown is in the way, or nothing is.
    fn backdrop(&self, x: f32, y: f32, under: usize, background: Option<Color>) -> Option<Color> {
        let mut between = Vec::new();
        let mut base = None;
        for fill in self.fills[..under].iter().rev() {
            match fill.at(x, y) {
                None => {}
                Some(Ink::Unknown) => return None,
                Some(Ink::Solid(color)) if color.a >= OPAQUE => {
                    base = Some(color);
                    break;
                }
                Some(Ink::Solid(color)) => between.push(color),
            }
        }
        let base = base.or(background.map(|color| color.with_alpha(1.0)))?;
        Some(
            between
                .into_iter()
                .rev()
                .fold(base, |under, top| top.over(under)),
        )
    }
}

fn centre(rect: Rect) -> (f32, f32) {
    (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

/// Whether `inner` lies inside `outer`, give or take half a pixel.
fn within(inner: Rect, outer: Rect) -> bool {
    inner.x + EPSILON >= outer.x
        && inner.y + EPSILON >= outer.y
        && inner.x + inner.width <= outer.x + outer.width + EPSILON
        && inner.y + inner.height <= outer.y + outer.height + EPSILON
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

#[cfg(all(test, feature = "previews"))]
#[path = "a11y.test.rs"]
mod tests;
