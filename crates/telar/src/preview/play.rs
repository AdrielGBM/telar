//! Play functions: scripted interactions against a mounted preview, each step logged with the frame it left behind.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::time::Duration;

use reactive_core::{OwnerId, dispose_owner, owner_scope};
use ui_core::focus;

use crate::{
    AccessNode, BuildFailure, Color, Event, EventResult, Key, LayoutError, LayoutItem, LayoutStyle,
    ModifiersState, NamedKey, Size, SurfaceCanvas, Text, TextStyle, motion, testing,
};

use super::a11y::{self, Severity};
use super::host::{Args, mount_preview};
use super::{ActionLog, Frame, PreviewCtx, PreviewEntry, Query};

/// A mounted preview a play drives: what [`PreviewEntry::play`] is handed, and what a host or a test mounts to run it.
///
/// The preview is mounted the way a workshop canvas mounts it — on a surface of its own, inside an error boundary, on its page — so its focus, overlays and environment are its own and a play sees what a person would. Each action finds its target in the accessibility snapshot, as a reader would, dispatches what a person's input would, and [settles](Self::settle) before it returns. Each action and assertion is logged as a [`PlayStep`] with the [`Frame`] it left behind.
///
/// ```ignore
/// preview!(button: ButtonProps, "Counting presses", |p| counting_button(p))
///     .play(|canvas| {
///         canvas.click(by_role(Role::Button).named("Pressed 0 times"))?;
///         canvas.expect_text("Pressed 1 times")?;
///         canvas.expect_action("on_press", 1)
///     })
/// ```
pub struct Play {
    entry: PreviewEntry,
    ctx: PreviewCtx,
    canvas: Option<Rc<SurfaceCanvas>>,
    failures: CaughtFailures,
    steps: Vec<PlayStep>,
    owner: Option<OwnerId>,
}

/// What the error boundary of a canvas a [`Play`] drives caught, so the play fails the step that broke the build. A host that mounts the preview its own way records each failure its boundary reports here, and hands the same handle to [`Play::attach`].
#[derive(Clone, Debug, Default)]
pub struct CaughtFailures(Rc<RefCell<Vec<String>>>);

impl CaughtFailures {
    pub fn record(&self, failure: &BuildFailure) {
        self.0.borrow_mut().push(failure.to_string());
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.borrow_mut())
    }
}

/// One thing a play did or asserted, how it went, and the canvas as it left it.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct PlayStep {
    /// What was done, as a panel lists it: `click button "Save"`.
    pub action: String,
    pub outcome: PlayResult,
    pub frame: Frame,
}

/// Why a play function stopped.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayError {
    pub message: String,
}

impl PlayError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PlayError {}

pub type PlayResult = Result<(), PlayError>;

pub type PlayFn = fn(&mut Play) -> PlayResult;

/// The most passes [`Play::settle`] takes for the animations a step started to reach their ends: one finishing can start the next.
const SETTLE_ROUNDS: usize = 32;

/// How far a value read may stray from the one a play expects: values a widget keeps in `f32` reach a reader widened to `f64`.
const VALUE_TOLERANCE: f64 = 1e-4;

/// How many moves [`Play::drag`] makes on its way: enough that a widget tracking the pointer sees it travel past a drag threshold rather than jump.
const DRAG_STEPS: u32 = 8;

impl Play {
    /// The canvas size a preview that names no viewport is mounted at.
    pub const DEFAULT_VIEWPORT: Size = Size::new(800.0, 600.0);

    /// Mounts `entry` with its args at their defaults, held in memory only, so a play starts from the same canvas whatever ran before it; at its viewport, or [`DEFAULT_VIEWPORT`](Self::DEFAULT_VIEWPORT).
    pub fn mount(entry: &PreviewEntry) -> Result<Self, PlayError> {
        Self::mount_in_owner(entry, Self::DEFAULT_VIEWPORT, || {
            PreviewCtx::for_entry_with(entry, Args::in_memory_for(entry))
        })
    }

    /// Mounts `entry` against `ctx`, sharing its args, action log and globals with whoever made it — a workshop running a play on the canvas its controls set. `viewport` is the size while the globals name none.
    pub fn mount_with(
        entry: &PreviewEntry,
        ctx: PreviewCtx,
        viewport: Size,
    ) -> Result<Self, PlayError> {
        Self::mount_in_owner(entry, viewport, || ctx)
    }

    /// Drives `canvas`, on which a host already mounted `entry` against `ctx` its own way — a workshop canvas, with its own card for a build that fails — rather than mounting one; the host's failure handler records into `failures`.
    ///
    /// The canvas is left as it is, neither resized nor settled, so a host runs the play once the canvas has the size it shows it at. A failure recorded before [`run`](Self::run) fails the run.
    pub fn attach(
        entry: &PreviewEntry,
        ctx: PreviewCtx,
        canvas: Rc<SurfaceCanvas>,
        failures: CaughtFailures,
    ) -> Self {
        Self {
            entry: *entry,
            ctx,
            canvas: Some(canvas),
            failures,
            steps: Vec::new(),
            owner: None,
        }
    }

    fn mount_in_owner(
        entry: &PreviewEntry,
        viewport: Size,
        ctx: impl FnOnce() -> PreviewCtx,
    ) -> Result<Self, PlayError> {
        crate::install_default_text_metrics();
        let scope = owner_scope();
        let owner = scope.id();
        let ctx = ctx();
        let failures = CaughtFailures::default();
        let canvas = canvas(entry, &ctx, viewport, &failures);
        drop(scope);
        let mut play = Self {
            entry: *entry,
            ctx,
            canvas: None,
            failures,
            steps: Vec::new(),
            owner: Some(owner),
        };
        play.canvas = Some(canvas?);
        play.settle_now();
        play.built()?;
        Ok(play)
    }

    /// Runs the entry's play function, if it has one, against this canvas.
    ///
    /// An error the function returns without a step having failed — a check of its own — is logged as a last step, so the log always ends on why the play stopped.
    pub fn run(&mut self) -> PlayResult {
        let Some(play) = self.entry.play else {
            return Ok(());
        };
        let args = self.ctx.args().clone();
        let outcome = args.driven(|| self.built().and_then(|()| play(self)));
        if let Err(error) = &outcome
            && self
                .steps
                .last()
                .is_none_or(|step| step.outcome.as_ref().err() != Some(error))
        {
            let frame = self.frame();
            self.steps.push(PlayStep {
                action: String::from("play"),
                outcome: outcome.clone(),
                frame,
            });
        }
        outcome
    }

    pub fn entry(&self) -> &PreviewEntry {
        &self.entry
    }

    /// The canvas's args, action log and globals.
    pub fn ctx(&self) -> &PreviewCtx {
        &self.ctx
    }

    pub fn actions(&self) -> ActionLog {
        self.ctx.actions()
    }

    /// The surface the preview is mounted on, for a host that shows or inspects it.
    pub fn canvas(&self) -> &Rc<SurfaceCanvas> {
        self.canvas
            .as_ref()
            .expect("a play holds its canvas until it drops")
    }

    /// Every step so far, oldest first.
    pub fn steps(&self) -> &[PlayStep] {
        &self.steps
    }

    pub fn into_steps(mut self) -> Vec<PlayStep> {
        std::mem::take(&mut self.steps)
    }

    /// The canvas as it is now.
    pub fn frame(&self) -> Frame {
        Frame::capture(self.canvas())
    }

    /// What a reader is told about the canvas now, in reading order.
    pub fn snapshot(&self) -> Vec<AccessNode> {
        self.canvas().access_snapshot()
    }

    /// The node `query` matches, or why there is not exactly one.
    pub fn find(&self, query: &Query) -> Result<AccessNode, PlayError> {
        let snapshot = self.snapshot();
        match query.find_all(&snapshot).as_slice() {
            [node] => Ok((*node).clone()),
            [] => Err(none_found(query, &snapshot)),
            many => Err(PlayError::new(format!(
                "{query} matches {} nodes; narrow it with `named`, `containing` or `nth`",
                many.len()
            ))),
        }
    }

    /// Every node `query` matches, in reading order.
    pub fn find_all(&self, query: &Query) -> Vec<AccessNode> {
        let snapshot = self.snapshot();
        query.find_all(&snapshot).into_iter().cloned().collect()
    }

    /// The [`a11y::check`] findings for the canvas now, without the rules the entry ignores.
    pub fn check_a11y(&self) -> a11y::Report {
        let frame = self.frame();
        let snapshot = frame
            .access
            .as_ref()
            .map(|access| access.snapshot.as_slice())
            .unwrap_or_default();
        a11y::check(&frame, snapshot).ignoring(self.entry.a11y_ignore)
    }

    /// Moves the pointer onto the node `query` matches, presses and releases it there.
    pub fn click(&mut self, query: Query) -> PlayResult {
        let outcome = self.enabled_target(&query).map(|node| {
            self.press_at(node.rect);
            self.settle_now();
        });
        self.step(format!("click {query}"), outcome)
    }

    /// Presses and releases the pointer on the canvas beside the node `query` matches, in the widest room left around it, as a person taps outside a panel to put it away. What lies there is not looked for: behind a modal layer a reader finds nothing.
    pub fn click_outside(&mut self, query: Query) -> PlayResult {
        let size = self.canvas().size();
        let outcome = self
            .find(&query)
            .and_then(|node| {
                room_outside(node.rect, size).ok_or_else(|| {
                    PlayError::new(format!("{query} leaves no room outside it on the canvas"))
                })
            })
            .map(|room| {
                self.press_at(room);
                self.settle_now();
            });
        self.step(format!("click outside {query}"), outcome)
    }

    /// Moves the pointer onto the node `query` matches.
    pub fn hover(&mut self, query: Query) -> PlayResult {
        let outcome = self.find(&query).map(|node| {
            let (x, y) = testing::centre(node.rect);
            self.dispatch(&testing::moved(x, y));
            self.settle_now();
        });
        self.step(format!("hover {query}"), outcome)
    }

    /// Takes the play's pointer off the canvas, as a hand moving away would, so whatever it left hovered settles back. Not a step: a canvas a person goes on to use after the play shows no hover but theirs, while a snapshot keeps the canvas as the play left it and never calls this.
    pub fn let_go(&mut self) {
        self.dispatch(&Event::CursorLeft);
        self.settle_now();
    }

    /// Presses on the node `query` matches, moves the pointer `dx`, `dy` logical px away in even steps, and releases it there, as a person drags a thumb, a divider or a row.
    pub fn drag(&mut self, query: Query, dx: f32, dy: f32) -> PlayResult {
        let outcome = self.enabled_target(&query).map(|node| {
            let (x, y) = testing::centre(node.rect);
            let (dx, dy) = (f64::from(dx), f64::from(dy));
            self.dispatch(&testing::moved(x, y));
            self.dispatch(&testing::press(x, y));
            for step in 1..=DRAG_STEPS {
                let along = f64::from(step) / f64::from(DRAG_STEPS);
                self.dispatch(&testing::moved(x + dx * along, y + dy * along));
            }
            self.dispatch(&testing::release(x + dx, y + dy));
            self.settle_now();
        });
        self.step(format!("drag {query} by ({dx}, {dy})"), outcome)
    }

    /// Clicks the node `query` matches, then types `text` into it a key at a time.
    pub fn type_text(&mut self, query: Query, text: &str) -> PlayResult {
        let outcome = self.enabled_target(&query).map(|node| {
            self.press_at(node.rect);
            for c in text.chars() {
                self.key(Key::Char(c), ModifiersState::default());
            }
            self.settle_now();
        });
        self.step(format!("type {text:?} into {query}"), outcome)
    }

    /// Presses and releases `key` wherever the keyboard is.
    pub fn press(&mut self, key: Key) -> PlayResult {
        self.key(key.clone(), ModifiersState::default());
        self.settle_now();
        self.step(format!("press {}", key_name(&key)), Ok(()))
    }

    /// Moves focus forward a stop, as Tab does.
    pub fn tab(&mut self) -> PlayResult {
        self.tab_towards(false);
        self.step(String::from("tab"), Ok(()))
    }

    /// Moves focus back a stop, as Shift+Tab does.
    pub fn tab_back(&mut self) -> PlayResult {
        self.tab_towards(true);
        self.step(String::from("shift+tab"), Ok(()))
    }

    /// Runs what the last input left pending — posted tasks, animations, layout — to its end. Every action settles before it returns; this is for what a play changes by other means, such as an arg.
    pub fn settle(&mut self) -> PlayResult {
        self.settle_now();
        self.step(String::from("settle"), Ok(()))
    }

    /// Lets `by` pass on the timer clock, runs every [`run_after`](crate::run_after) that came due — a notice that puts itself away, a tooltip that opens once the pointer rests — and settles what they changed, without the play waiting it out. Nothing else a play does fires a timer, so one fires where the play says.
    pub fn advance(&mut self, by: Duration) -> PlayResult {
        testing::advance_time(by);
        self.settle_now();
        self.step(format!("advance {by:?}"), Ok(()))
    }

    /// That `query` matches something.
    pub fn expect(&mut self, query: Query) -> PlayResult {
        let outcome = self.find_some(&query);
        self.step(format!("expect {query}"), outcome)
    }

    /// That `query` matches nothing.
    pub fn expect_absent(&mut self, query: Query) -> PlayResult {
        let found = self.find_all(&query).len();
        let outcome = match found {
            0 => Ok(()),
            n => Err(PlayError::new(format!("expected no {query}, found {n}"))),
        };
        self.step(format!("expect no {query}"), outcome)
    }

    /// That the canvas draws `text`, as a part of one string it draws.
    pub fn expect_text(&mut self, text: &str) -> PlayResult {
        let frame = self.frame();
        let outcome = if frame.texts().any(|drawn| drawn.contains(text)) {
            Ok(())
        } else {
            Err(PlayError::new(format!(
                "expected the text {text:?}; the canvas draws {:?}",
                frame.texts().collect::<Vec<_>>()
            )))
        };
        self.step(format!("expect text {text:?}"), outcome)
    }

    /// That the canvas draws `text` nowhere.
    pub fn expect_no_text(&mut self, text: &str) -> PlayResult {
        let frame = self.frame();
        let outcome = match frame.texts().find(|drawn| drawn.contains(text)) {
            None => Ok(()),
            Some(drawn) => Err(PlayError::new(format!(
                "expected no text {text:?}, but the canvas draws {drawn:?}"
            ))),
        };
        self.step(format!("expect no text {text:?}"), outcome)
    }

    /// That the node `query` matches holds focus.
    pub fn expect_focused(&mut self, query: Query) -> PlayResult {
        let outcome = self.find(&query).and_then(|node| {
            if node.focused {
                return Ok(());
            }
            let holder = self.snapshot().into_iter().find(|node| node.focused);
            Err(PlayError::new(match holder {
                Some(holder) => {
                    format!("expected {query} to hold focus; {} does", describe(&holder))
                }
                None => format!("expected {query} to hold focus; nothing does"),
            }))
        });
        self.step(format!("expect focus on {query}"), outcome)
    }

    /// That the node `query` matches is in its on state — checked, pressed, selected or expanded, as its role says — or out of it.
    pub fn expect_toggled(&mut self, query: Query, on: bool) -> PlayResult {
        let outcome = self.find(&query).and_then(|node| match node.toggled {
            Some(state) if state == on => Ok(()),
            Some(state) => Err(PlayError::new(format!(
                "expected {query} to be {}, but it is {}",
                on_off(on),
                on_off(state)
            ))),
            None => Err(PlayError::new(format!(
                "expected {query} to be {}, but it has no on/off state",
                on_off(on)
            ))),
        });
        self.step(format!("expect {query} {}", on_off(on)), outcome)
    }

    /// That the node `query` matches is expanded — its popup open, its branch shown — or collapsed.
    pub fn expect_expanded(&mut self, query: Query, expanded: bool) -> PlayResult {
        let word = |expanded| if expanded { "expanded" } else { "collapsed" };
        let outcome = self.find(&query).and_then(|node| match node.expanded {
            Some(state) if state == expanded => Ok(()),
            Some(state) => Err(PlayError::new(format!(
                "expected {query} to be {}, but it is {}",
                word(expanded),
                word(state)
            ))),
            None => Err(PlayError::new(format!(
                "expected {query} to be {}, but it neither expands nor collapses",
                word(expanded)
            ))),
        });
        self.step(format!("expect {query} {}", word(expanded)), outcome)
    }

    /// That the node `query` matches reads `value` now, as a slider or a spin button tells a reader.
    pub fn expect_value(&mut self, query: Query, value: f64) -> PlayResult {
        let outcome = self.find(&query).and_then(|node| match node.value {
            Some(read) if (read.now - value).abs() <= VALUE_TOLERANCE => Ok(()),
            Some(read) => Err(PlayError::new(format!(
                "expected {query} to read {value}, but it reads {}",
                read.now
            ))),
            None => Err(PlayError::new(format!(
                "expected {query} to read {value}, but it has no value"
            ))),
        });
        self.step(format!("expect {query} = {value}"), outcome)
    }

    /// That the preview's callback `name` was called `count` times, by the calls its [`ActionLog`] keeps.
    pub fn expect_action(&mut self, name: &str, count: usize) -> PlayResult {
        let actions = self.actions();
        let seen = actions.count_of(name);
        let outcome = if seen == count {
            Ok(())
        } else {
            let logged: Vec<&str> = actions.calls().iter().map(|call| call.name).collect();
            Err(PlayError::new(format!(
                "expected {count} call(s) to `{name}`, saw {seen}; the log holds {logged:?}"
            )))
        };
        self.step(format!("expect {count} × {name}"), outcome)
    }

    /// That [`check_a11y`](Self::check_a11y) finds no [`Severity::Error`].
    pub fn expect_accessible(&mut self) -> PlayResult {
        let report = self.check_a11y();
        let errors: Vec<String> = report
            .at_least(Severity::Error)
            .map(ToString::to_string)
            .collect();
        let outcome = if errors.is_empty() {
            Ok(())
        } else {
            Err(PlayError::new(format!(
                "the canvas is not accessible:\n{}",
                errors.join("\n")
            )))
        };
        self.step(String::from("expect accessible"), outcome)
    }

    fn step(&mut self, action: String, outcome: PlayResult) -> PlayResult {
        let outcome = outcome.and_then(|()| self.built());
        let frame = self.frame();
        self.steps.push(PlayStep {
            action,
            outcome: outcome.clone(),
            frame,
        });
        outcome
    }

    /// Fails with what the preview's boundary caught since this was last asked, so a build that fails mid-play fails the step that caused it.
    fn built(&self) -> PlayResult {
        match self.failures.take().first() {
            None => Ok(()),
            Some(failure) => Err(PlayError::new(format!("the preview failed: {failure}"))),
        }
    }

    fn find_some(&self, query: &Query) -> PlayResult {
        let snapshot = self.snapshot();
        if query.find_all(&snapshot).is_empty() {
            Err(none_found(query, &snapshot))
        } else {
            Ok(())
        }
    }

    fn enabled_target(&self, query: &Query) -> Result<AccessNode, PlayError> {
        let node = self.find(query)?;
        if node.enabled {
            Ok(node)
        } else {
            Err(PlayError::new(format!("{query} is disabled")))
        }
    }

    fn dispatch(&self, event: &Event) -> EventResult {
        self.canvas().dispatch_own(event)
    }

    /// A primary press and release at the centre of `rect`, with the pointer moved there first, and focus let go first where the press lands on nothing that takes it, as a canvas frame does.
    fn press_at(&self, rect: crate::Rect) {
        let (x, y) = testing::centre(rect);
        self.dispatch(&testing::moved(x, y));
        self.canvas().with_entered(|| {
            focus::blur_from_pointer(x as f32, y as f32);
        });
        self.dispatch(&testing::press(x, y));
        self.dispatch(&testing::release(x, y));
    }

    fn key(&self, key: Key, modifiers: ModifiersState) {
        self.dispatch(&testing::key_with(key.clone(), modifiers));
        self.dispatch(&Event::KeyReleased { key, modifiers });
    }

    /// Tab as a canvas frame answers it: the focused control first, then the canvas's own order when nothing inside holds focus.
    fn tab_towards(&self, backwards: bool) {
        let modifiers = ModifiersState {
            is_shift: backwards,
            ..ModifiersState::default()
        };
        let tab = testing::key_with(Key::Named(NamedKey::Tab), modifiers);
        if self.dispatch(&tab) != EventResult::Handled {
            self.canvas().with_entered(|| {
                if focus::current().is_none() {
                    if backwards {
                        focus::focus_prev();
                    } else {
                        focus::focus_next();
                    }
                }
            })
        }
        self.dispatch(&Event::KeyReleased {
            key: Key::Named(NamedKey::Tab),
            modifiers,
        });
        self.settle_now();
    }

    /// Runs posted tasks and every animation to its end, laying out what they change, the way a runner settles a page before its first frame.
    fn settle_now(&self) {
        let canvas = self.canvas();
        let scale = motion::scale();
        motion::set_scale(0.0);
        let now = web_time::Instant::now();
        for _ in 0..SETTLE_ROUNDS {
            reactive_core::drain_tasks();
            canvas.with_entered(|| {
                motion::tick(now);
            });
            canvas.relayout_if_dirty();
            if !motion::has_active() {
                break;
            }
        }
        motion::set_scale(scale);
    }
}

impl Drop for Play {
    fn drop(&mut self) {
        self.canvas.take();
        if let Some(owner) = self.owner {
            dispose_owner(owner);
        }
    }
}

impl fmt::Debug for Play {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Play")
            .field("entry", &self.entry)
            .field("steps", &self.steps.len())
            .finish_non_exhaustive()
    }
}

fn canvas(
    entry: &PreviewEntry,
    ctx: &PreviewCtx,
    viewport: Size,
    failures: &CaughtFailures,
) -> Result<Rc<SurfaceCanvas>, PlayError> {
    let (ctx, globals) = (ctx.clone(), ctx.globals());
    let record = failures.clone();
    let canvas = globals
        .canvas(viewport, move || {
            mount_preview(entry, ctx, move |failure| failed(&record, failure), None)
        })
        .map_err(|error| PlayError::new(format!("the canvas did not build: {error:?}")))?;
    canvas.resize(globals.viewport().peek().unwrap_or(viewport));
    Ok(canvas)
}

fn failed(
    failures: &CaughtFailures,
    failure: BuildFailure,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    failures.record(&failure);
    let message = format!("Error: {failure}");
    Ok(Box::new(Text::new(
        move || message.clone(),
        LayoutStyle::new(),
        || TextStyle::new(12.0, Color::rgba(0.9, 0.2, 0.2, 1.0)),
    )?))
}

fn none_found(query: &Query, snapshot: &[AccessNode]) -> PlayError {
    let reads = match platform_core::accessibility::transcript(snapshot) {
        transcript if transcript.is_empty() => String::from("nothing"),
        transcript => transcript,
    };
    PlayError::new(format!("found no {query}; the canvas reads:\n{reads}"))
}

/// The widest strip of a canvas of `size` beside `rect`, left of it, right of it, above or below; `None` when `rect` leaves no whole pixel on any side.
fn room_outside(rect: crate::Rect, size: Size) -> Option<crate::Rect> {
    let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
    [
        crate::Rect::new(0.0, 0.0, rect.x, size.height),
        crate::Rect::new(right, 0.0, size.width - right, size.height),
        crate::Rect::new(0.0, 0.0, size.width, rect.y),
        crate::Rect::new(0.0, bottom, size.width, size.height - bottom),
    ]
    .into_iter()
    .filter(|strip| strip.width >= 1.0 && strip.height >= 1.0)
    .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
}

fn describe(node: &AccessNode) -> String {
    format!("{} {:?}", node.role.as_str(), node.name)
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn key_name(key: &Key) -> String {
    match key {
        Key::Char(c) => format!("{c:?}"),
        Key::Named(named) => format!("{named:?}"),
    }
}

#[cfg(test)]
#[path = "play_test.rs"]
mod tests;
