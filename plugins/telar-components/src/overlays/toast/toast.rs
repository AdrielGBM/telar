//! [`toaster`]: the stack of short notices an application posts with [`show_toast`] or [`Toasts::push`], each put away when its time is up, its action is pressed or it is dismissed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use telar::{
    Accessible, AlignItems, Border, BorderRadius, BoxText, Container, FixedLayer, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, Props, Reactive, ReactiveList, RectStyle, RwSignal,
    ShapeStyle, StyledContainer, SurfaceAmendment, Text, TextWrap, Timer, amend_surface, box_item,
    children, detached, effect, focus, focus::Role, on_cleanup, run_after, signal, style_follows,
};

use crate::shared;
use crate::strings;
use telar::{ControlSize, Tone};

const DISMISS_GLYPH: &str = "✕";

/// A notice's own tokens, which an application's theme supplies through a `#[theme(extension)]` field.
///
/// Every field left `None` follows the theme role or measure named beside it, read when the notice paints and lays out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToastTokens {
    /// Corner radius of each notice. `None` is the theme's `radius_surface`.
    pub radius: Option<f32>,
    /// Space between a notice's edge and what it says, at the regular size, scaled with the toaster's own. `None` is [`metrics::gap_lg`](crate::metrics::gap_lg).
    pub pad: Option<f32>,
    /// A notice's width when the toaster names none, at the regular size, scaled with the toaster's text. `None` is 26 ems of the document's text.
    pub width: Option<f32>,
    /// Width of the stripe in the colour of the notice's tone, at the regular size, scaled with the toaster's own. `None` is [`metrics::gap_xs`](crate::metrics::gap_xs).
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

    /// The space between a notice's edge and what it says in a toaster of `size`.
    pub fn pad(&self, size: ControlSize) -> f32 {
        self.pad
            .map_or_else(|| shared::gap_lg(size), |pad| pad * size.scale())
    }

    /// A notice's width in a toaster of `size` that names none.
    pub fn width(&self, size: ControlSize) -> f32 {
        shared::width_or_measure(size, self.width, 26.0)
    }

    /// The stripe's width in a toaster of `size`.
    pub fn stripe_width(&self, size: ControlSize) -> f32 {
        self.stripe_width
            .map_or_else(|| shared::gap_xs(size), |width| width * size.scale())
    }
}

/// A button on a notice that does something about it: "Undo", "Retry". Pressing it also puts the notice away.
#[derive(Clone)]
pub struct ToastAction {
    pub label: Arc<str>,
    pub run: Rc<dyn Fn()>,
}

/// What a notice is about: the colour of its stripe, the word a reader hears before it, and whether it interrupts what a reader is saying.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToastTone {
    /// Worth knowing, with nothing to do about it.
    #[default]
    Info,
    /// Done, or gone well.
    Success,
    /// Worth a look before it becomes a problem. Interrupts a reader.
    Warning,
    /// Failed. Interrupts a reader.
    Error,
}

impl ToastTone {
    /// The theme's colour of this intent, which the stripe is drawn in.
    pub fn tone(self) -> Tone {
        match self {
            ToastTone::Info => Tone::Info,
            ToastTone::Success => Tone::Success,
            ToastTone::Warning => Tone::Warning,
            ToastTone::Error => Tone::Danger,
        }
    }

    /// The role a notice of this tone takes: [`Role::Alert`], announced at once over what a reader is saying, for a warning or an error; `None` for one heard when the reader is next quiet, as everything in the stack's status is.
    pub fn role(self) -> Option<Role> {
        match self {
            ToastTone::Info | ToastTone::Success => None,
            ToastTone::Warning | ToastTone::Error => Some(Role::Alert),
        }
    }

    /// The word a reader hears before the notice, in the locale in force.
    fn word(self) -> String {
        strings::text(match self {
            ToastTone::Info => strings::INFORMATION,
            ToastTone::Success => strings::SUCCESS,
            ToastTone::Warning => strings::WARNING,
            ToastTone::Error => strings::ERROR,
        })
    }
}

/// What puts a notice away, besides its dismiss button.
#[derive(Clone)]
pub enum ToastEnd {
    /// Its time being up: it shows this long, not counting the time the pointer rests on the stack, the keyboard spends on its buttons or a modal layer is up.
    After(Duration),
    /// Nothing: it stays until it is dismissed, for one that asks something of the reader.
    Dismissed,
    /// Its action being pressed. Never its time being up, since reaching the button can take a reader longer than any notice lasts.
    Action(ToastAction),
}

/// One notice: what it says, what it is about and what puts it away.
#[derive(Clone)]
pub struct Toast {
    pub message: Arc<str>,
    pub title: Option<Arc<str>>,
    /// A new notice is [`ToastTone::Info`].
    pub tone: ToastTone,
    /// A new notice puts itself away after [`DEFAULT_DURATION`](Self::DEFAULT_DURATION).
    pub end: ToastEnd,
}

impl Toast {
    /// How long a notice shows unless it says otherwise.
    pub const DEFAULT_DURATION: Duration = Duration::from_secs(5);

    pub fn new(message: impl Into<Arc<str>>) -> Self {
        Self {
            message: message.into(),
            title: None,
            tone: ToastTone::Info,
            end: ToastEnd::After(Self::DEFAULT_DURATION),
        }
    }

    pub fn titled(mut self, title: impl Into<Arc<str>>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn tone(mut self, tone: ToastTone) -> Self {
        self.tone = tone;
        self
    }

    /// Puts the notice away after `duration`, in place of whatever put it away before, an action included.
    pub fn lasting(mut self, duration: Duration) -> Self {
        self.end = ToastEnd::After(duration);
        self
    }

    /// Keeps the notice until it is dismissed, for one that asks something of the reader.
    pub fn until_dismissed(mut self) -> Self {
        self.end = ToastEnd::Dismissed;
        self
    }

    /// Gives the notice a button that runs `run` and puts it away, and keeps the notice until that is pressed or it is dismissed.
    pub fn with_action(mut self, label: impl Into<Arc<str>>, run: impl Fn() + 'static) -> Self {
        self.end = ToastEnd::Action(ToastAction {
            label: label.into(),
            run: Rc::new(run),
        });
        self
    }

    /// What a reader hears for the notice: its tone's word, then its title and its message.
    fn spoken(&self) -> String {
        let tone = self.tone.word();
        match &self.title {
            Some(title) => strings::text_with(
                strings::TITLED_NOTICE,
                &[
                    ("tone", &tone),
                    ("title", title),
                    ("message", &self.message),
                ],
            ),
            None => strings::text_with(
                strings::NOTICE,
                &[("tone", &tone), ("message", &self.message)],
            ),
        }
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

/// Posts `toast` to the shared queue: `show_toast("Saved")`, or a [`Toast`] built up with a title, a tone and an action.
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

/// Where the notices of a [`Toasts`] queue show: a stack against one edge of the surface, over the page and out of its scroll, announced to a reader as each one arrives, a dialog or a menu up or not.
///
/// Mount one per surface. A notice on a clock puts itself away when its time is up, and the pointer resting on the stack, the keyboard on one of its buttons or a modal layer up holds every clock until it leaves. Notices past [`max_visible`](Self::max_visible) wait their turn rather than push the oldest out unread.
#[derive(Props)]
pub struct ToasterProps {
    /// The queue to show. `None` (the default) shows the shared one [`show_toast`] posts to.
    #[props(some, default)]
    pub toasts: Option<Toasts>,
    #[props(default)]
    pub placement: ToastPlacement,
    /// How many notices show at once. Three by default.
    #[props(default = 3)]
    pub max_visible: u32,
    /// Each notice's width in logical px. `None` (the default) is [`ToastTokens::width`].
    #[props(some, default)]
    pub width: Option<f32>,
    /// How large the notices are drawn: their padding, text, buttons and the gaps between them, and the width they take when the toaster names none, as [`ControlSize`] works them out.
    #[props(default)]
    pub size: ControlSize,
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
        size,
        label,
        style,
    } = props;
    let queue = queue.unwrap_or_else(toasts);
    let max_visible = max_visible as usize;
    let width = Width { named: width, size };
    let held = Rc::new(Hold::default());
    effect({
        let held = held.clone();
        move || held.modal(focus::modal_layer_up())
    });

    let shown = move || {
        let mut shown: Vec<Posted> = queue
            .posted
            .with(|posted| posted.iter().take(max_visible).cloned().collect());
        if placement.is_top() {
            shown.reverse();
        }
        shown
    };
    let stack_box = move || LayoutStyle::column().gap(shared::gap_md(size));
    let stack = ReactiveList::with_style(stack_box(), shown, |entry: &Posted| entry.id, {
        let held = held.clone();
        move |entry| notice(size, queue, entry, width, &style, &held)
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
            .padding_all(shared::gap_xl(size))
    };
    let layer = FixedLayer::new(layer_box(), children![region])?;
    style_follows(layer.layout_node(), layer_box);
    Ok(box_item(layer))
}

/// The width a toaster was given, else its theme's at its size.
#[derive(Clone, Copy)]
struct Width {
    named: Option<f32>,
    size: ControlSize,
}

impl Width {
    fn get(self) -> f32 {
        self.named
            .unwrap_or_else(|| ToastTokens::current().width(self.size))
    }
}

/// What holds the clock of every shown notice, so the one being read or acted on never closes under it: the pointer resting on the stack, the keyboard on one of its buttons, or a modal layer up that keeps the keyboard and a reader from reaching it.
#[derive(Default)]
struct Hold {
    pointer: Cell<bool>,
    keyboard: Cell<bool>,
    modal: Cell<bool>,
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

    fn modal(&self, held: bool) {
        self.modal.set(held);
        self.apply();
    }

    fn is_held(&self) -> bool {
        self.pointer.get() || self.keyboard.get() || self.modal.get()
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
    size: ControlSize,
    queue: Toasts,
    entry: Posted,
    width: Width,
    style: &Option<SurfaceAmendment>,
    held: &Rc<Hold>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Posted { id, toast } = entry;
    let dismiss = Rc::new(move || queue.dismiss(id));

    if let ToastEnd::After(duration) = toast.end {
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
            move |t| shared::control_text(size, t, 1.0).with_font_weight(shared::strong_weight()),
        )?));
    }
    let message = toast.message.clone();
    words.push(box_item(Text::declaring(
        move || message.to_string(),
        LayoutStyle::new(),
        move |t| shared::control_text(size, t, 1.0).with_text_wrap(TextWrap::Wrap),
    )?));
    let column = move || {
        LayoutStyle::column()
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .min_width(0.0)
            .gap(shared::gap_xs(size))
    };
    let text = Container::new(column(), words)?
        .styled_by(column)
        .a11y_label({
            let toast = toast.clone();
            move || toast.spoken()
        });

    let mut parts: Vec<Box<dyn LayoutItem>> =
        vec![stripe(size, toast.tone.tone())?, box_item(text)];
    if let ToastEnd::Action(action) = toast.end.clone() {
        let dismiss = dismiss.clone();
        let label = action.label.clone();
        parts.push(text_button(
            size,
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
        size,
        || DISMISS_GLYPH.to_string(),
        move || dismiss(),
        held,
        true,
    )?);

    let card_box = move || {
        LayoutStyle::row()
            .align_items(AlignItems::START)
            .width(width.get())
            .padding_all(ToastTokens::current().pad(size))
            .gap(shared::gap_md(size))
    };
    let style = style.clone();
    let card = StyledContainer::new(
        card_box(),
        move |_r| amend_surface(card_paint(), &style),
        parts,
    )?
    .styled_by(card_box);
    Ok(box_item(match toast.tone.role() {
        Some(role) => card.role(role),
        None => card,
    }))
}

/// A notice's paint before the caller's amendment.
fn card_paint() -> RectStyle {
    RectStyle::default()
        .with_fill(shared::surface())
        .with_border(Border::uniform(shared::border(), shared::border_width()))
        .with_radius(BorderRadius::all(ToastTokens::current().radius()))
}

fn stripe(size: ControlSize, tone: Tone) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let stripe_box = move || {
        LayoutStyle::new()
            .width(ToastTokens::current().stripe_width(size))
            .align_self_stretch()
            .flex_shrink(0.0)
    };
    let text = signal(BoxText::initial());
    let stripe = StyledContainer::new(
        stripe_box(),
        move |r| {
            RectStyle::default()
                .with_fill(tone.color(text.get().ink))
                .with_radius(shared::pill(r))
        },
        vec![],
    )?
    .following_text(text)
    .styled_by(stripe_box)
    .a11y_hidden();
    Ok(box_item(stripe))
}

/// A small button of text on a notice: its action, or the glyph that dismisses it, which a reader hears by name instead.
fn text_button(
    size: ControlSize,
    words: impl Fn() -> String + 'static,
    press: impl Fn() + 'static,
    held: &Rc<Hold>,
    dismisses: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(words, LayoutStyle::new(), move |t| {
        let t = match dismisses {
            true => shared::quiet(size, t, 1.0),
            false => shared::control_text(size, t, 1.0)
                .with_color(shared::accent())
                .with_font_weight(shared::strong_weight()),
        };
        t.with_text_wrap(TextWrap::NoWrap)
    })?;
    let shell = move || {
        LayoutStyle::row()
            .align_items(AlignItems::CENTER)
            .flex_shrink(0.0)
            .padding_horizontal(shared::gap_sm(size))
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
#[path = "toast.test.rs"]
mod tests;
