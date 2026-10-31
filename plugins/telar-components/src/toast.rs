//! [`toaster`]: the stack of short notices an application posts with [`show_toast`] or [`Toasts::push`], each of which puts itself away after a while.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, Color, Container, FixedLayer, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, Props, Reactive, ReactiveList, RectStyle, RwSignal,
    ShapeStyle, StyledContainer, SurfaceAmendment, Text, TextWrap, Timer, amend_surface, box_item,
    children, detached, focus::Role, on_cleanup, run_after, signal, style_follows,
};

use crate::shared;
use crate::strings;

const DISMISS_GLYPH: &str = "✕";

/// A notice's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the notice paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToastTokens {
    /// Corner radius of each notice. `None` is the theme's `radius_surface`.
    pub radius: Option<f32>,
    /// Space between a notice's edge and what it says, at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_lg`](crate::metrics::gap_lg).
    pub pad: Option<f32>,
    /// A notice's width when the toaster names none, at the regular control size, scaled with the control size in force. `None` is 26 ems of the document's text.
    pub width: Option<f32>,
    /// Width of the stripe in the colour of the notice's kind, at the regular control size, scaled with the control size in force. `None` is [`metrics::gap_xs`](crate::metrics::gap_xs).
    pub stripe_width: Option<f32>,
}

impl ToastTokens {
    /// The tokens of the theme in force, subscribing the caller to it.
    pub fn current() -> Self {
        telar::use_theme_extension::<Self>()
    }

    pub fn radius(&self) -> f32 {
        self.radius.unwrap_or_else(shared::radius_surface)
    }

    pub fn pad(&self) -> f32 {
        self.pad
            .map_or_else(shared::gap_lg, |pad| pad * telar::control_scale())
    }

    pub fn width(&self) -> f32 {
        shared::width_or_measure(self.width, 26.0)
    }

    pub fn stripe_width(&self) -> f32 {
        self.stripe_width
            .map_or_else(shared::gap_xs, |width| width * telar::control_scale())
    }
}

/// What a notice is about, which decides the colour of its stripe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn accent(self) -> Color {
        let tokens = telar::use_theme_tokens();
        match self {
            ToastKind::Info => tokens.info(),
            ToastKind::Success => tokens.success(),
            ToastKind::Warning => tokens.warning(),
            ToastKind::Error => tokens.error(),
        }
    }
}

/// A button on a notice that does something about it: "Undo", "Retry". Pressing it also puts the notice away.
#[derive(Clone)]
pub struct ToastAction {
    pub label: Arc<str>,
    pub run: Rc<dyn Fn()>,
}

/// One notice: what it says, what it is about, how long it stays and what can be done about it.
#[derive(Clone)]
pub struct Toast {
    pub message: Arc<str>,
    pub title: Option<Arc<str>>,
    pub kind: ToastKind,
    /// How long it shows before putting itself away, not counting the time the pointer rests on the stack or the keyboard spends on its buttons. `None` keeps it until it is dismissed.
    pub duration: Option<Duration>,
    pub action: Option<ToastAction>,
}

impl Toast {
    /// How long a notice shows unless it says otherwise.
    pub const DEFAULT_DURATION: Duration = Duration::from_secs(5);

    pub fn new(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            title: None,
            kind: ToastKind::default(),
            duration: Some(Self::DEFAULT_DURATION),
            action: None,
        }
    }

    pub fn titled(mut self, title: impl Into<Arc<str>>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn lasting(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Keeps the notice until it is dismissed, for one that asks something of the reader.
    pub fn until_dismissed(mut self) -> Self {
        self.duration = None;
        self
    }

    pub fn with_action(mut self, label: impl Into<Arc<str>>, run: impl Fn() + 'static) -> Self {
        self.action = Some(ToastAction {
            label: label.into(),
            run: Rc::new(run),
        });
        self
    }
}

impl From<&str> for Toast {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<String> for Toast {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

/// Names one posted notice, to take it down early with [`Toasts::dismiss`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToastId(u64);

#[derive(Clone)]
struct Posted {
    id: ToastId,
    toast: Rc<Toast>,
}

/// A queue of notices and the handle to post into it: copy it anywhere, and whatever [`toaster`] shows it puts each notice up.
///
/// [`toasts`] is the one every toaster shows unless told otherwise, so most code never makes one: it calls [`show_toast`].
#[derive(Clone, Copy)]
pub struct Toasts {
    posted: RwSignal<Vec<Posted>>,
}

telar::dropless_thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
    static SHARED: Cell<Option<Toasts>> = const { Cell::new(None) };
}

impl Toasts {
    /// An empty queue of its own, for a toaster kept apart from the shared one.
    pub fn new() -> Self {
        Self {
            posted: detached(|| signal(Vec::new())),
        }
    }

    /// Posts `toast` and hands back the id that takes it down early.
    pub fn push(self, toast: impl Into<Toast>) -> ToastId {
        let id = ToastId(NEXT_ID.with(|next| next.replace(next.get() + 1)));
        let toast = Rc::new(toast.into());
        self.posted
            .update(|posted| posted.push(Posted { id, toast }));
        id
    }

    /// Takes the notice down, shown or still waiting its turn. Nothing happens when it is already gone.
    pub fn dismiss(self, id: ToastId) {
        if self
            .posted
            .peek_with(|posted| posted.iter().any(|entry| entry.id == id))
        {
            self.posted
                .update(|posted| posted.retain(|entry| entry.id != id));
        }
    }

    pub fn clear(self) {
        self.posted.set(Vec::new());
    }

    /// How many notices are posted and not yet taken down, shown or waiting. Reactive.
    pub fn len(self) -> usize {
        self.posted.with(Vec::len)
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    fn is_alive(self) -> bool {
        self.posted.is_alive()
    }
}

impl Default for Toasts {
    fn default() -> Self {
        Self::new()
    }
}

/// The queue every [`toaster`] shows unless it is handed one of its own, minted on first use.
pub fn toasts() -> Toasts {
    SHARED.with(|shared| match shared.get() {
        Some(toasts) if toasts.is_alive() => toasts,
        _ => {
            let toasts = Toasts::new();
            shared.set(Some(toasts));
            toasts
        }
    })
}

/// Posts `toast` to the shared queue: `show_toast("Saved")`, or a [`Toast`] built up with a title, a kind and an action.
pub fn show_toast(toast: impl Into<Toast>) -> ToastId {
    toasts().push(toast)
}

/// Which corner or edge of the surface the stack stands against. The newest notice is always the one nearest the edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToastPlacement {
    TopStart,
    TopCenter,
    TopEnd,
    BottomStart,
    BottomCenter,
    #[default]
    BottomEnd,
}

impl ToastPlacement {
    fn is_top(self) -> bool {
        matches!(
            self,
            ToastPlacement::TopStart | ToastPlacement::TopCenter | ToastPlacement::TopEnd
        )
    }

    fn across(self) -> AlignItems {
        match self {
            ToastPlacement::TopStart | ToastPlacement::BottomStart => AlignItems::START,
            ToastPlacement::TopCenter | ToastPlacement::BottomCenter => AlignItems::CENTER,
            ToastPlacement::TopEnd | ToastPlacement::BottomEnd => AlignItems::END,
        }
    }
}

/// Where the notices of a [`Toasts`] queue show: a stack against one edge of the surface, over the page and out of its scroll, announced to a reader as each one arrives.
///
/// Mount one per surface. Each notice puts itself away when its time is up, and the pointer resting on the stack or the keyboard on one of its buttons holds every clock until it leaves. Notices past [`max_visible`](Self::max_visible) wait their turn rather than push the oldest out unread.
#[derive(Props)]
pub struct ToasterProps {
    /// The queue to show. `None` (the default) shows the shared one [`show_toast`] posts to.
    #[props(some, default)]
    pub toasts: Option<Toasts>,
    #[props(default)]
    pub placement: ToastPlacement,
    /// How many notices show at once. `0` means 3.
    #[props(default)]
    pub max_visible: u32,
    /// Each notice's width in logical px. `0.0` (the default) means "unset" -> [`ToastTokens::width`].
    #[props(default)]
    pub width: f32,
    /// What a reader calls the stack. `None` (the default) uses the localized `telar_components.notifications` message.
    #[props(some, into, default)]
    pub label: Option<Reactive<String>>,
    /// Amends the paint of each notice's card — its **principal surface**. See [`telar::SurfaceStyle`] for when a theme token or [`ToastTokens`] is the right instrument instead.
    #[props(some, default)]
    pub style: Option<SurfaceAmendment>,
}

/// A stack of notices against one edge of the surface, each putting itself away after a while.
pub fn toaster(props: ToasterProps) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ToasterProps {
        toasts: queue,
        placement,
        max_visible,
        width,
        label,
        style,
    } = props;
    let queue = queue.unwrap_or_else(toasts);
    let max_visible = if max_visible == 0 {
        3
    } else {
        max_visible as usize
    };
    let width = Width((width > 0.0).then_some(width));
    let held = Rc::new(Hold::default());

    let shown = move || {
        let mut shown: Vec<Posted> = queue
            .posted
            .with(|posted| posted.iter().take(max_visible).cloned().collect());
        if placement.is_top() {
            shown.reverse();
        }
        shown
    };
    let stack_box = || LayoutStyle::column().gap(shared::gap_md());
    let stack = ReactiveList::with_style(stack_box(), shown, |entry: &Posted| entry.id, {
        let held = held.clone();
        move |entry| notice(queue, entry, width, &style, &held)
    })?;
    style_follows(stack.layout_node(), stack_box);
    let region_box = move || LayoutStyle::column().width(width.get());
    let region = StyledContainer::new(region_box(), |_r| RectStyle::default(), children![stack])?
        .styled_by(region_box)
        .on_hover({
            let held = held.clone();
            move |inside| held.pointer(inside)
        })
        .role(Role::Status)
        .a11y_label(move || match &label {
            Some(label) => label.get(),
            None => strings::text(strings::NOTIFICATIONS),
        });

    let layer_box = move || {
        LayoutStyle::column()
            .align_items(placement.across())
            .justify_content(if placement.is_top() {
                JustifyContent::START
            } else {
                JustifyContent::END
            })
            .padding_all(shared::gap_xl())
    };
    let layer = FixedLayer::new(layer_box(), children![region])?;
    style_follows(layer.layout_node(), layer_box);
    Ok(box_item(layer))
}

/// The width a toaster was given, else its theme's.
#[derive(Clone, Copy)]
struct Width(Option<f32>);

impl Width {
    fn get(self) -> f32 {
        self.0.unwrap_or_else(|| ToastTokens::current().width())
    }
}

/// What holds the clock of every shown notice, so the one being read or acted on never closes under it: the pointer resting on the stack, or the keyboard on one of its buttons.
#[derive(Default)]
struct Hold {
    pointer: Cell<bool>,
    keyboard: Cell<bool>,
    clocks: RefCell<Vec<(ToastId, Timer)>>,
}

impl Hold {
    fn pointer(&self, held: bool) {
        self.pointer.set(held);
        self.apply();
    }

    fn keyboard(&self, held: bool) {
        self.keyboard.set(held);
        self.apply();
    }

    fn is_held(&self) -> bool {
        self.pointer.get() || self.keyboard.get()
    }

    fn apply(&self) {
        let held = self.is_held();
        for (_, clock) in self.clocks.borrow().iter() {
            if held {
                clock.pause();
            } else {
                clock.resume();
            }
        }
    }

    fn track(&self, id: ToastId, clock: Timer) {
        if self.is_held() {
            clock.pause();
        }
        self.clocks.borrow_mut().push((id, clock));
    }

    fn forget(&self, id: ToastId) {
        self.clocks.borrow_mut().retain(|(at, _)| *at != id);
    }
}

fn notice(
    queue: Toasts,
    entry: Posted,
    width: Width,
    style: &Option<SurfaceAmendment>,
    held: &Rc<Hold>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Posted { id, toast } = entry;
    let dismiss = Rc::new(move || queue.dismiss(id));

    if let Some(duration) = toast.duration {
        let clock = run_after(duration, {
            let dismiss = dismiss.clone();
            move || dismiss()
        });
        held.track(id, clock);
        let held = held.clone();
        on_cleanup(move || held.forget(id));
    }

    let mut words: Vec<Box<dyn LayoutItem>> = Vec::new();
    if let Some(title) = toast.title.clone() {
        words.push(box_item(Text::declaring(
            move || title.to_string(),
            LayoutStyle::new(),
            |t| shared::control_text(t, 1.0).with_font_weight(shared::strong_weight()),
        )?));
    }
    let message = toast.message.clone();
    words.push(box_item(Text::declaring(
        move || message.to_string(),
        LayoutStyle::new(),
        |t| shared::control_text(t, 1.0).with_text_wrap(TextWrap::Wrap),
    )?));
    let column = || {
        LayoutStyle::column()
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .min_width(0.0)
            .gap(shared::gap_xs())
    };
    let text = Container::new(column(), words)?.styled_by(column);

    let mut parts: Vec<Box<dyn LayoutItem>> = vec![stripe(toast.kind)?, box_item(text)];
    if let Some(action) = toast.action.clone() {
        let dismiss = dismiss.clone();
        let label = action.label.clone();
        parts.push(text_button(
            move || label.to_string(),
            move || {
                (action.run)();
                dismiss();
            },
            held,
            false,
        )?);
    }
    parts.push(text_button(
        || DISMISS_GLYPH.to_string(),
        move || dismiss(),
        held,
        true,
    )?);

    let card_box = move || {
        LayoutStyle::row()
            .align_items(AlignItems::START)
            .width(width.get())
            .padding_all(ToastTokens::current().pad())
            .gap(shared::gap_md())
    };
    let style = style.clone();
    let card = StyledContainer::new(
        card_box(),
        move |_r| amend_surface(card_paint(), &style),
        parts,
    )?
    .styled_by(card_box);
    Ok(box_item(card))
}

/// A notice's paint before the caller's amendment.
fn card_paint() -> RectStyle {
    RectStyle::default()
        .with_fill(shared::surface())
        .with_border(Border::uniform(shared::border(), shared::border_width()))
        .with_radius(BorderRadius::all(ToastTokens::current().radius()))
}

fn stripe(kind: ToastKind) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let stripe_box = || {
        LayoutStyle::new()
            .width(ToastTokens::current().stripe_width())
            .align_self_stretch()
            .flex_shrink(0.0)
    };
    let stripe = StyledContainer::new(
        stripe_box(),
        move |r| {
            RectStyle::default()
                .with_fill(kind.accent())
                .with_radius(shared::pill(r))
        },
        vec![],
    )?
    .styled_by(stripe_box)
    .a11y_hidden();
    Ok(box_item(stripe))
}

/// A small button of text on a notice: its action, or the glyph that dismisses it, which a reader hears by name instead.
fn text_button(
    words: impl Fn() -> String + 'static,
    press: impl Fn() + 'static,
    held: &Rc<Hold>,
    dismisses: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(words, LayoutStyle::new(), move |t| {
        let t = shared::control_text(t, 1.0).with_text_wrap(TextWrap::NoWrap);
        if dismisses {
            shared::quiet(t, 1.0)
        } else {
            t.with_color(shared::accent())
                .with_font_weight(shared::strong_weight())
        }
    })?;
    let shell = || {
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .padding_horizontal(shared::gap_sm())
    };
    let shape = || RectStyle::default().with_radius(BorderRadius::all(shared::radius_item()));
    let focused = Rc::new(Cell::new(false));
    {
        let (held, focused) = (held.clone(), focused.clone());
        on_cleanup(move || {
            if focused.get() {
                held.keyboard(false);
            }
        });
    }
    let held = held.clone();
    let button = StyledContainer::new(shell(), move |_r| shape(), children![text])?
        .styled_by(shell)
        .hover_style(move |_r| shape().with_fill(shared::hover_wash()))
        .active_style(move |_r| shape().with_fill(shared::pressed_wash()))
        .control(Role::Button)
        .on_focus(move |now| {
            focused.set(now);
            held.keyboard(now);
        })
        .on_press(press);
    Ok(box_item(if dismisses {
        button.a11y_label(|| strings::text(strings::DISMISS))
    } else {
        button
    }))
}

#[cfg(test)]
#[path = "toast_test.rs"]
mod tests;
