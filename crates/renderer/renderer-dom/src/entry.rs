//! Typing, from an input method that will not talk to a canvas.
//!
//! Telar owns the text: the field is its widget, the caret is its own, and the value lives in a signal. What it does not own is *how characters are produced*. A browser gives that job to an editable element, and only to an editable element — a soft keyboard rises for one, an input method composes into one, a password manager fills one, dictation writes into one. A page that draws its fields itself gets none of that, and there is no key event to substitute for it: an IME reports `keydown` with a key that means "ask the input method", and the text arrives later, as a composition, addressed to a field.
//!
//! So there is one, parked at the caret of whichever field holds the keyboard and invisible on top of it, which is where an input method opens its candidates. It is the element a screen reader reads, since it is the one that is focused, so it mirrors the field: an `<input>` for a field of one line and a `<textarea>` for one of several, holding what the field shows with the field's selection, written again after every change to the field. It never changes on its own account. Every edit the browser reports is prevented before it lands and goes back through the platform's own event queue as the keys the field would have received anyway, which is what lets the whole editing path — selection, undo, the caret — stay exactly where it was; the field's next frame writes the result back.
//!
//! A composition is the one thing it holds for a while: the browser composes into the element, over the field's selection, and the entry reports each step of it to the field as [`Ime::Preedit`] and its end as one [`Ime::Commit`], swallowing whatever a browser goes on to report as an edit of its own, so a composed word is typed once.

use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use platform_core::consumed_keys::CONSUMED_KEYS_ATTRIBUTE;
use platform_core::{ConsumedKeys, Event, Ime, Key, ModifiersState, NamedKey};
use renderer_core::Editing;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::paint;

/// Off the page, not `display:none`: a hidden element cannot be focused, and one with no size gives an input method nowhere to put its candidate window. It is over the field and invisible instead. One line that never wraps, so what is composed into it runs along the caret's line and the candidates follow it there.
const HIDDEN: &str = "position:absolute;padding:0;border:0;outline:none;background:transparent;\
color:transparent;caret-color:transparent;font:inherit;resize:none;overflow:hidden;\
white-space:pre;-webkit-user-select:text;user-select:text;";

/// The `keyCode` a browser gives a key an input method is handling, even after it says the composition is over: Safari reports the Enter that confirms one as a fresh key.
const PROCESSED_BY_INPUT_METHOD: u32 = 229;

/// Marks the entry, which holds the keyboard on behalf of a field box without being one.
pub(crate) const ENTRY_ATTRIBUTE: &str = "data-telar-entry";

/// The field an entry is parked over, as the reconcile found it in the document.
pub struct Field<'a> {
    pub rect: geometry_core::Rect,
    /// Where the field's caret is, in the same coordinates as `rect`; `None` when the field said nothing, and the entry covers the whole field instead.
    pub caret: Option<Rect>,
    /// What the field shows and what is selected in it; `None` when it said nothing, and the entry shows nothing.
    pub shows: Option<&'a Editing>,
    pub multiline: bool,
    pub label: Option<&'a str>,
    pub keys: Option<&'a str>,
    pub element: Option<web_sys::HtmlElement>,
}

type Listener = (
    web_sys::EventTarget,
    &'static str,
    bool,
    Closure<dyn FnMut(web_sys::Event)>,
);

type Standing = Rc<RefCell<Option<web_sys::HtmlElement>>>;

/// What the entry shows: the field's text with its selection, both ends in UTF-16 code units, as a browser counts them.
#[derive(Clone, Default, PartialEq, Debug)]
struct Shown {
    text: String,
    anchor: u32,
    focus: u32,
}

impl Shown {
    fn of(editing: &Editing) -> Self {
        let text = editing.text();
        let units = |at: usize| text[..at].encode_utf16().count() as u32;
        Self {
            text: text.to_string(),
            anchor: units(editing.anchor()),
            focus: units(editing.focus()),
        }
    }

    fn range(&self) -> (u32, u32) {
        (self.anchor.min(self.focus), self.anchor.max(self.focus))
    }

    /// How many code units the selection leaves before it and after it: what a composition over it keeps on either side.
    fn around_selection(&self) -> (usize, usize) {
        let length = self.text.encode_utf16().count();
        let (start, end) = self.range();
        let start = (start as usize).min(length);
        (start, length - (end as usize).clamp(start, length))
    }

    /// This, with the selection replaced by `text` and the caret after it.
    fn replacing_selection(&self, text: &str) -> Self {
        let units: Vec<u16> = self.text.encode_utf16().collect();
        let (before, after) = self.around_selection();
        let mut replaced = units[..before].to_vec();
        replaced.extend(text.encode_utf16());
        let caret = replaced.len() as u32;
        replaced.extend_from_slice(&units[units.len() - after..]);
        Self {
            text: String::from_utf16_lossy(&replaced),
            anchor: caret,
            focus: caret,
        }
    }

    /// What `value` puts in place of the selection, when it is this with only the selection replaced.
    fn inserted_by(&self, value: &str) -> Option<String> {
        let (before, after) = self.around_selection();
        let units: Vec<u16> = self.text.encode_utf16().collect();
        let written: Vec<u16> = value.encode_utf16().collect();
        let kept = written.len().checked_sub(before + after)?;
        let same = written[..before] == units[..before]
            && written[before + kept..] == units[units.len() - after..];
        same.then(|| String::from_utf16_lossy(&written[before..before + kept]))
    }
}

/// The entry's element, as the kind of editable element it is.
enum Editable {
    Line(web_sys::HtmlInputElement),
    Lines(web_sys::HtmlTextAreaElement),
}

impl Editable {
    fn of(node: &web_sys::EventTarget) -> Option<Self> {
        if let Some(line) = node.dyn_ref::<web_sys::HtmlInputElement>() {
            return Some(Self::Line(line.clone()));
        }
        node.dyn_ref::<web_sys::HtmlTextAreaElement>()
            .map(|lines| Self::Lines(lines.clone()))
    }

    fn value(&self) -> String {
        match self {
            Self::Line(line) => line.value(),
            Self::Lines(lines) => lines.value(),
        }
    }

    fn set_value(&self, value: &str) {
        match self {
            Self::Line(line) => line.set_value(value),
            Self::Lines(lines) => lines.set_value(value),
        }
    }

    /// The selection as `(start, end, direction)`, when the element has one.
    fn selection(&self) -> Option<(u32, u32, String)> {
        let (start, end, direction) = match self {
            Self::Line(line) => (
                line.selection_start(),
                line.selection_end(),
                line.selection_direction(),
            ),
            Self::Lines(lines) => (
                lines.selection_start(),
                lines.selection_end(),
                lines.selection_direction(),
            ),
        };
        Some((
            start.ok()??,
            end.ok()??,
            direction.ok()?.unwrap_or_default(),
        ))
    }

    fn select(&self, start: u32, end: u32, direction: &str) {
        let _ = match self {
            Self::Line(line) => line.set_selection_range_with_direction(start, end, direction),
            Self::Lines(lines) => lines.set_selection_range_with_direction(start, end, direction),
        };
    }

    /// Makes the element show `shown`, writing only what differs: a value written again moves the browser's caret, and a reader announces every move.
    fn show(&self, shown: &Shown) {
        if self.value() != shown.text {
            self.set_value(&shown.text);
        }
        let (start, end) = shown.range();
        let direction = match shown.anchor > shown.focus {
            true => "backward",
            false => "forward",
        };
        let collapsed = start == end;
        let current = self.selection();
        let same = current.as_ref().is_some_and(|(at, to, going)| {
            (*at, *to) == (start, end) && (collapsed || going == direction)
        });
        if !same {
            self.select(start, end, direction);
        }
    }
}

/// A composition the browser is making in the entry, as far as the field has been told of it.
#[derive(Default)]
struct Composing {
    active: bool,
    /// The field it began in. Its steps reach the field only while the entry still stands for that one.
    began_in: Option<web_sys::HtmlElement>,
    /// How many code units of what the entry showed it keeps before and after itself: the selection it was typed over is what lies between.
    around: (usize, usize),
    /// A press landed while it was open: one that moves the browser's focus ends it, and the field the press takes the keyboard from types what it showed by itself.
    pressed: bool,
    /// The last step the field was told of, so a step reported twice is told once.
    told: Option<Ime>,
    /// The text the last composition ended with, which a browser may go on to insert as an edit of its own.
    committed: Option<String>,
}

impl Composing {
    fn belongs_to(&self, standing: &Standing) -> bool {
        self.began_in.is_some() && same(self.began_in.as_ref(), standing.borrow().as_ref())
    }
}

/// The hidden editable element the browser types into, placed at the caret of whichever field holds the keyboard and showing what that field shows.
pub struct TextEntry {
    node: web_sys::HtmlElement,
    /// Taken off their targets when the entry goes, since the element can outlive it: a later reconcile on the same host takes it over.
    listeners: Vec<Listener>,
    /// Where it is parked, so an unchanged frame writes nothing.
    placed: String,
    /// The name it is currently answering to, for the same reason.
    named: Option<String>,
    /// Whether it currently stands for a field, so it can be taken out of the way when none does.
    active: bool,
    /// The keys the field it stands for keeps, as last written.
    keys: Option<String>,
    /// The field it stands for, which is where the browser's own Tab order has to start from.
    field: Standing,
    /// What the field it stands for shows, which the element is brought back to whenever no composition holds it.
    shown: Rc<RefCell<Shown>>,
    composing: Rc<RefCell<Composing>>,
}

impl TextEntry {
    /// The entry `host` already holds, when an earlier reconcile left one there, or a new one appended after everything the host was served with.
    pub fn new(document: &web_sys::Document, host: &web_sys::HtmlElement) -> Option<Self> {
        let existing = host
            .query_selector(&format!(":scope > [{ENTRY_ATTRIBUTE}]"))
            .ok()
            .flatten()
            .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            .filter(|node| Editable::of(node.as_ref()).is_some());
        let created = existing.is_none();
        let node = match existing {
            Some(node) => node,
            None => editable(document, false)?,
        };
        prepare(&node);
        stand_by(&node, true);
        if created {
            host.append_child(node.as_ref()).ok()?;
        }

        let field: Standing = Rc::default();
        let shown: Rc<RefCell<Shown>> = Rc::default();
        let composing: Rc<RefCell<Composing>> = Rc::default();
        let target: &web_sys::EventTarget = node.as_ref();
        let listeners = vec![
            listen(target, "keydown", false, {
                let (field, composing) = (field.clone(), composing.clone());
                move |event| on_key_down(event, &field, &composing)
            }),
            listen(target, "beforeinput", false, {
                let composing = composing.clone();
                move |event| on_before_input(event, &composing)
            }),
            listen(target, "input", false, {
                let (field, shown, composing) = (field.clone(), shown.clone(), composing.clone());
                move |event| on_input(event, &field, &shown, &composing)
            }),
            listen(target, "compositionstart", false, {
                let (field, shown, composing) = (field.clone(), shown.clone(), composing.clone());
                move |_| on_composition_start(&field, &shown, &composing)
            }),
            listen(target, "compositionupdate", false, {
                let (field, composing) = (field.clone(), composing.clone());
                move |event| on_composition_update(event, &field, &composing)
            }),
            listen(target, "compositionend", false, {
                let (field, shown, composing) = (field.clone(), shown.clone(), composing.clone());
                move |event| on_composition_end(event, &field, &shown, &composing)
            }),
            // Capturing, so it is heard before the press moves the browser's focus and that move ends the composition.
            listen(host.as_ref(), "pointerdown", true, {
                let composing = composing.clone();
                move |_| {
                    let mut composing = composing.borrow_mut();
                    composing.pressed = composing.active;
                }
            }),
        ];
        Some(Self {
            node,
            listeners,
            placed: String::new(),
            named: None,
            active: false,
            keys: None,
            field,
            shown,
            composing,
        })
    }

    /// Parks the entry at the caret of the field that holds the keyboard, shows what the field shows, and takes the keyboard with it.
    ///
    /// Focus goes here rather than to the field's own element, and that is the point: the element a person sees is not one a browser will type into, and the one it will type into must not be seen.
    ///
    /// A screen reader reads the *focused* element, which is this one and never the field, so it is what the field is to a reader: one line or several, a password or not, the field's text and selection, and the field's own name. A field nobody named leaves it unnamed rather than answering to a name the app never gave.
    ///
    /// It also carries the keys the field keeps, since the platform reads them off whichever element the key was sent to. `take_focus` is false when a person moved focus elsewhere on the page: the entry still follows the field, and leaves the keyboard where it went.
    pub fn park(&mut self, field: Field<'_>, take_focus: bool) {
        let Field {
            rect,
            caret,
            shows,
            multiline,
            label,
            keys,
            element,
        } = field;
        if is_multiline(&self.node) != multiline {
            self.become_editable(multiline);
        }
        let mut style = String::from(HIDDEN);
        let at = caret.map_or(rect, |caret| along_the_caret(rect, caret));
        paint::declare(&mut style, "left", &paint::px(at.x));
        paint::declare(&mut style, "top", &paint::px(at.y));
        paint::declare(&mut style, "width", &paint::px(at.width.max(1.0)));
        paint::declare(&mut style, "height", &paint::px(at.height.max(1.0)));
        if caret.is_some() {
            paint::declare(&mut style, "line-height", &paint::px(at.height.max(1.0)));
        }
        if style != self.placed {
            let _ = self.node.set_attribute("style", &style);
            self.placed = style;
        }
        let name = label.filter(|label| !label.is_empty());
        if self.named.as_deref() != name {
            crate::adopt::patch_attribute(&self.node, "aria-label", name);
            self.named = name.map(str::to_string);
        }
        if !multiline {
            let secret = shows.is_some_and(Editing::secret);
            crate::adopt::patch_attribute(
                &self.node,
                "type",
                Some(if secret { "password" } else { "text" }),
            );
        }
        crate::adopt::patch_attribute(
            &self.node,
            "enterkeyhint",
            Some(if multiline { "enter" } else { "done" }),
        );
        if self.keys.as_deref() != keys {
            crate::adopt::patch_attribute(&self.node, CONSUMED_KEYS_ATTRIBUTE, keys);
            self.keys = keys.map(str::to_string);
        }
        *self.field.borrow_mut() = element;
        *self.shown.borrow_mut() = shows.map(Shown::of).unwrap_or_default();
        // A press that left the keyboard in the field it was composing in did not end the composition.
        let composing = {
            let mut composing = self.composing.borrow_mut();
            if composing.active && composing.belongs_to(&self.field) {
                composing.pressed = false;
            }
            composing.active
        };
        // The browser owns the element while it composes: writing to it would end the composition under the input method.
        if !composing && let Some(editable) = Editable::of(self.node.as_ref()) {
            editable.show(&self.shown.borrow());
        }
        self.active = true;
        stand_by(&self.node, false);
        if take_focus && !self.holds_focus() {
            let _ = self.node.focus();
        }
    }

    /// Takes the entry out of the way, for a frame where no field holds the keyboard.
    ///
    /// Focus that moved on to whatever claimed it is left there. Focus still here has nowhere it was claimed for, and an empty editable element is no place to leave a reader, so it is let go.
    pub fn release(&mut self) {
        if self.holds_focus() {
            let _ = self.node.blur();
        }
        if !std::mem::take(&mut self.active) {
            return;
        }
        let _ = self.node.set_attribute("style", HIDDEN);
        self.placed.clear();
        *self.field.borrow_mut() = None;
        // Standing for no field, it must not keep answering to the last one, nor keep what it showed.
        if self.named.take().is_some() {
            let _ = self.node.remove_attribute("aria-label");
        }
        if self.keys.take().is_some() {
            let _ = self.node.remove_attribute(CONSUMED_KEYS_ATTRIBUTE);
        }
        *self.shown.borrow_mut() = Shown::default();
        if let Some(editable) = Editable::of(self.node.as_ref()) {
            editable.show(&Shown::default());
        }
        stand_by(&self.node, true);
    }

    /// Puts the entry back where the reconcile expects it, after a frame that swept the host of anything past the boxes it placed. It is a child of the host and not of the page so that the coordinates it is parked at are the ones every other box uses.
    pub fn settle(&self, host: &web_sys::HtmlElement, at: u32) {
        let node: &web_sys::Node = self.node.as_ref();
        let current = host.child_nodes().item(at);
        if current
            .as_ref()
            .is_some_and(|existing| existing.is_same_node(Some(node)))
        {
            return;
        }
        let _ = host.insert_before(node, current.as_ref());
    }

    /// Swaps the element for the kind a field of `multiline` lines is, since an element's tag cannot change: the listeners move to the new one, and what the old one was told is told again.
    fn become_editable(&mut self, multiline: bool) {
        let Some(node) = self
            .node
            .owner_document()
            .and_then(|document| editable(&document, multiline))
        else {
            return;
        };
        prepare(&node);
        let _ = self.node.replace_with_with_node_1(node.as_ref());
        let new: &web_sys::EventTarget = node.as_ref();
        for (target, event, capture, closure) in &mut self.listeners {
            let on_old = target
                .dyn_ref::<web_sys::Node>()
                .is_some_and(|target| target.is_same_node(Some(self.node.as_ref())));
            if !on_old {
                continue;
            }
            let callback = closure.as_ref().unchecked_ref();
            let _ = target.remove_event_listener_with_callback_and_bool(event, callback, *capture);
            let _ = new.add_event_listener_with_callback_and_bool(event, callback, *capture);
            *target = new.clone();
        }
        *self.composing.borrow_mut() = Composing::default();
        self.node = node;
        self.placed.clear();
        self.named = None;
        self.keys = None;
    }

    fn holds_focus(&self) -> bool {
        is_focused(&self.node)
    }
}

/// A new editable element of the kind a field of `multiline` lines is.
fn editable(document: &web_sys::Document, multiline: bool) -> Option<web_sys::HtmlElement> {
    let tag = if multiline { "textarea" } else { "input" };
    document.create_element(tag).ok()?.dyn_into().ok()
}

fn is_multiline(node: &web_sys::HtmlElement) -> bool {
    node.dyn_ref::<web_sys::HtmlTextAreaElement>().is_some()
}

/// What the entry says about itself whatever field it stands for.
fn prepare(node: &web_sys::HtmlElement) {
    let multiline = is_multiline(node);
    let attributes = [
        ("style", Some(HIDDEN)),
        (ENTRY_ATTRIBUTE, Some("")),
        ("autocapitalize", Some("off")),
        ("autocorrect", Some("off")),
        ("autocomplete", Some("off")),
        ("spellcheck", Some("false")),
        // One row as far as the browser is concerned: the rows are Telar's to lay out, and what is composed into it runs along the caret's line.
        ("rows", multiline.then_some("1")),
        ("type", (!multiline).then_some("text")),
        // Deliberately not `aria-hidden`: this is where the keyboard actually is, and a focused element a reader has been told to ignore is worse than one it reads.
        ("tabindex", Some("-1")),
    ];
    for (name, value) in attributes {
        crate::adopt::patch_attribute(node, name, value);
    }
}

/// Where the entry stands for a field whose caret is at `caret`: on the caret's line, from the caret to the field's far edge, so a composition typed into it runs where the field draws it.
fn along_the_caret(field: Rect, caret: Rect) -> Rect {
    let right = field.x + field.width;
    Rect::new(
        caret.x,
        caret.y,
        (right - caret.x).max(caret.height),
        caret.height,
    )
}

fn same(a: Option<&web_sys::HtmlElement>, b: Option<&web_sys::HtmlElement>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.is_same_node(Some(b.as_ref())),
        (None, None) => true,
        _ => false,
    }
}

/// Takes the entry out of the document while it stands for no field, and puts it back for one. Standing for none it is neither a control a reader should find nor content the page's landmarks hold. Never while it holds focus, which hiding it would take away.
fn stand_by(node: &web_sys::HtmlElement, idle: bool) {
    if idle && is_focused(node) {
        return;
    }
    crate::adopt::patch_attribute(node, "hidden", idle.then_some(""));
}

fn is_focused(node: &web_sys::HtmlElement) -> bool {
    node.owner_document()
        .and_then(|document| document.active_element())
        .is_some_and(|active| active.is_same_node(Some(node.as_ref())))
}

fn listen(
    target: &web_sys::EventTarget,
    event: &'static str,
    capture: bool,
    handler: impl Fn(&web_sys::Event) + 'static,
) -> Listener {
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        handler(&event);
    });
    let _ = target.add_event_listener_with_callback_and_bool(
        event,
        closure.as_ref().unchecked_ref(),
        capture,
    );
    (target.clone(), event, capture, closure)
}

impl Drop for TextEntry {
    fn drop(&mut self) {
        for (target, event, capture, closure) in &self.listeners {
            let _ = target.remove_event_listener_with_callback_and_bool(
                event,
                closure.as_ref().unchecked_ref(),
                *capture,
            );
        }
    }
}

/// The edits a browser reports before making them, as the keys the field would have received.
///
/// `beforeinput` rather than `keydown` because this is where the ones a key never named arrive: a deletion from a soft keyboard, a line dictated, a paste. The default is prevented in every case — the element shows what the field shows, and only the field's next frame changes that.
fn on_before_input(event: &web_sys::Event, composing: &RefCell<Composing>) {
    let Some(event) = event.dyn_ref::<web_sys::InputEvent>() else {
        return;
    };
    // A composition in progress is not an edit yet: the input method is still deciding, and its end is reported as a commit.
    if event.is_composing() || composing.borrow().active {
        return;
    }
    let kind = event.input_type();
    match kind.as_str() {
        // What a browser inserts once a composition is over, on top of the `compositionend` that already committed it.
        "insertCompositionText" | "insertFromComposition" => {
            event.prevent_default();
            let text = event.data().unwrap_or_default();
            let echo = composing.borrow().committed.as_deref() == Some(text.as_str());
            if !echo {
                post_text(&text);
            }
        }
        "insertText" | "insertReplacementText" | "insertFromPaste" => {
            event.prevent_default();
            let Some(text) = event.data() else {
                return;
            };
            post_text(&text);
        }
        "insertLineBreak" | "insertParagraph" => {
            event.prevent_default();
            post_key(Key::Named(NamedKey::Enter));
        }
        "deleteContentBackward" | "deleteWordBackward" | "deleteSoftLineBackward" => {
            event.prevent_default();
            post_key(Key::Named(NamedKey::Backspace));
        }
        "deleteContentForward" | "deleteWordForward" => {
            event.prevent_default();
            post_key(Key::Named(NamedKey::Delete));
        }
        // Everything else is an edit this field has no key for, a composition's own deletion among them. Prevented rather than let through, which would leave the element showing what the field does not.
        _ => event.prevent_default(),
    }
}

/// Each step of a composition the browser wrote into the element, and what an input method wrote there directly.
///
/// `beforeinput` prevents every edit, so outside a composition nothing lands here — except what an input method wrote directly, which no `beforeinput` precedes on every browser, and the text a composition ended with, which some browsers write again once it is over. What replaced the selection is typed, unless it is that echo; anything else puts the element back to what the field shows.
fn on_input(
    event: &web_sys::Event,
    field: &Standing,
    shown: &RefCell<Shown>,
    composing: &RefCell<Composing>,
) {
    let Some(target) = event.target().as_ref().and_then(Editable::of) else {
        return;
    };
    let in_composition = event
        .dyn_ref::<web_sys::InputEvent>()
        .is_some_and(web_sys::InputEvent::is_composing);
    if in_composition || composing.borrow().active {
        let around = composing.borrow().around;
        tell(field, composing, composed(&target, around));
        return;
    }
    let shown = shown.borrow().clone();
    let value = target.value();
    if value == shown.text {
        return;
    }
    let echo = composing.borrow_mut().committed.take();
    match shown.inserted_by(&value) {
        Some(inserted) if echo.as_deref() == Some(inserted.as_str()) => {}
        Some(inserted) if inserted.is_empty() => {
            post_key(Key::Named(NamedKey::Delete));
            return;
        }
        Some(inserted) => {
            post_text(&inserted);
            return;
        }
        None => {}
    }
    target.show(&shown);
}

/// The composition as the element holds it: what lies between the text it kept on either side, with the browser's selection inside it as the input method's cursor.
fn composed(target: &Editable, (before, after): (usize, usize)) -> Ime {
    let units: Vec<u16> = target.value().encode_utf16().collect();
    let start = before.min(units.len());
    let end = units.len().saturating_sub(after).max(start);
    let inside = |at: u32| (at as usize).clamp(start, end) as u32 - start as u32;
    let cursor = target
        .selection()
        .map(|(from, to, _)| (inside(from), inside(to)));
    Ime::preedit_utf16(String::from_utf16_lossy(&units[start..end]), cursor)
}

fn on_composition_start(field: &Standing, shown: &RefCell<Shown>, composing: &RefCell<Composing>) {
    let mut composing = composing.borrow_mut();
    *composing = Composing {
        active: true,
        began_in: field.borrow().clone(),
        around: shown.borrow().around_selection(),
        ..Composing::default()
    };
}

/// A step of the composition, with the caret after it: the `input` that follows says where the input method's caret really is, once the browser has written the step.
fn on_composition_update(event: &web_sys::Event, field: &Standing, composing: &RefCell<Composing>) {
    let Some(event) = event.dyn_ref::<web_sys::CompositionEvent>() else {
        return;
    };
    let text = event.data().unwrap_or_default();
    let end = text.len();
    tell(field, composing, Ime::preedit(text, Some((end, end))));
}

/// The composition's text, committed once, to the field it began in — unless the press that ended it took the keyboard from that field, which then types what it showed by itself, or the keyboard had already moved on.
///
/// The element is left showing the field with the text committed in place of its selection, which is what the field's next frame writes; a composition that was not the field's is taken back out.
fn on_composition_end(
    event: &web_sys::Event,
    field: &Standing,
    shown: &RefCell<Shown>,
    composing: &RefCell<Composing>,
) {
    let text = event
        .dyn_ref::<web_sys::CompositionEvent>()
        .and_then(web_sys::CompositionEvent::data)
        .unwrap_or_default();
    let ours = {
        let mut composing = composing.borrow_mut();
        let ours = composing.active && !composing.pressed && composing.belongs_to(field);
        *composing = Composing {
            committed: Some(text.clone()),
            ..Composing::default()
        };
        ours
    };
    if ours {
        let committed = shown.borrow().replacing_selection(&text);
        *shown.borrow_mut() = committed;
    }
    if let Some(target) = event.target().as_ref().and_then(Editable::of) {
        target.show(&shown.borrow());
    }
    if ours {
        platform_core::post_event(Event::Ime(Ime::Commit(text)));
    }
}

/// Tells the field the composition stands at `step`, once, and only while the entry still stands for the field it began in.
fn tell(field: &Standing, composing: &RefCell<Composing>, step: Ime) {
    let mut composing = composing.borrow_mut();
    if !composing.belongs_to(field) || composing.told.as_ref() == Some(&step) {
        return;
    }
    composing.told = Some(step.clone());
    drop(composing);
    platform_core::post_event(Event::Ime(step));
}

/// Text the browser inserted, as the keys that would have typed it.
fn post_text(text: &str) {
    for character in text.chars() {
        post_key(Key::Char(character));
    }
}

/// One key, pressed and released, as the field would have seen it.
///
/// Both halves because a widget is entitled to expect them in pairs: one that tracked a held key would otherwise believe every character typed here is still down.
fn post_key(key: Key) {
    let modifiers = ModifiersState::default();
    platform_core::post_event(Event::KeyPressed {
        key: key.clone(),
        modifiers,
        unmodified: None,
    });
    platform_core::post_event(Event::KeyReleased { key, modifiers });
}

/// Keeps a character from being typed twice.
///
/// While the entry holds the keyboard there are two ways for one keystroke to become text: the platform's own `keydown` listener on the host, and the `beforeinput` this element reports. Both fire, and the field received every letter twice.
///
/// The split is by what the two can each see. A printable character is exactly what `beforeinput` describes better — it is the same event whether it came from a key, a soft keyboard, an input method or dictation — so it stops here. Everything else, arrows and Escape and every shortcut, produces no input event at all, and goes on up to the platform as it always did.
///
/// A Tab the field does not keep is the browser's to walk, and the walk has to start from the field: the entry sits at the end of the host, so a walk from here would start past the app's last box. Handing focus to the field before the default runs is what makes the browser step from where the person sees the caret.
fn on_key_down(event: &web_sys::Event, field: &Standing, composing: &RefCell<Composing>) {
    let Some(event) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
        return;
    };
    // The keys an input method is in the middle of consuming are its own, whatever they say they are.
    if event.is_composing() || event.key_code() == PROCESSED_BY_INPUT_METHOD {
        event.stop_propagation();
        return;
    }
    composing.borrow_mut().committed = None;
    if event.key() == "Tab" && !field_keeps_tab(event) {
        if let Some(field) = field.borrow().as_ref() {
            let _ = field.focus();
        }
        return;
    }
    // A modifier held is a command, not a character, and commands stay the platform's.
    if event.ctrl_key() || event.meta_key() {
        return;
    }
    if event.key().chars().count() == 1 {
        event.stop_propagation();
    }
}

fn field_keeps_tab(event: &web_sys::KeyboardEvent) -> bool {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|element| element.get_attribute(CONSUMED_KEYS_ATTRIBUTE))
        .is_some_and(|keys| ConsumedKeys::from_names(&keys).contains(ConsumedKeys::TAB))
}
