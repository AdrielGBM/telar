//! Typing, from an input method that will not talk to a canvas.
//!
//! Telar owns the text: the field is its widget, the caret is its own, and the value lives in a signal. What it does not own is *how characters are produced*. A browser gives that job to an editable element, and only to an editable element — a soft keyboard rises for one, an input method composes into one, a password manager fills one, dictation writes into one. A page that draws its fields itself gets none of that, and there is no key event to substitute for it: an IME reports `keydown` with a key that means "ask the input method", and the text arrives later, as a composition, addressed to a field.
//!
//! So there is one, parked at the caret of whichever field holds the keyboard and invisible on top of it, which is where an input method opens its candidates. It never shows what was typed — Telar draws that — and it is emptied after every insertion, so it accumulates nothing and has no state to keep in step. What it produces goes back through the platform's own event queue as the keys the field would have received anyway, which is what lets the whole editing path — selection, undo, the caret — stay exactly where it was.
//!
//! A composition is the one thing it holds for a while: the browser composes into the element, and the entry reports each step of it to the field as [`Ime::Preedit`] and its end as one [`Ime::Commit`], swallowing whatever a browser goes on to report as an edit of its own, so a composed word is typed once.

use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use platform_core::consumed_keys::CONSUMED_KEYS_ATTRIBUTE;
use platform_core::{ConsumedKeys, Event, Ime, Key, ModifiersState, NamedKey};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::paint;

/// Off the page, not `display:none`: a hidden element cannot be focused, and one with no size gives an input method nowhere to put its candidate window. It is over the field and invisible instead. One line that never wraps, so what is composed into it runs along the caret's line and the candidates follow it there.
const HIDDEN: &str = "position:absolute;padding:0;border:0;outline:none;background:transparent;\
color:transparent;caret-color:transparent;font:inherit;resize:none;overflow:hidden;\
white-space:pre;-webkit-user-select:text;user-select:text;";

/// What the entry answers to while standing for a field that named nothing.
const FALLBACK: &str = "Text field";

/// The `keyCode` a browser gives a key an input method is handling, even after it says the composition is over: Safari reports the Enter that confirms one as a fresh key.
const PROCESSED_BY_INPUT_METHOD: u32 = 229;

/// Marks the entry, which holds the keyboard on behalf of a field box without being one.
pub(crate) const ENTRY_ATTRIBUTE: &str = "data-telar-entry";

/// The field an entry is parked over, as the reconcile found it in the document.
pub struct Field<'a> {
    pub rect: geometry_core::Rect,
    /// Where the field's caret is, in the same coordinates as `rect`; `None` when the field said nothing, and the entry covers the whole field instead.
    pub caret: Option<Rect>,
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

/// A composition the browser is making in the entry, as far as the field has been told of it.
#[derive(Default)]
struct Composing {
    active: bool,
    /// The field it began in. Its steps reach the field only while the entry still stands for that one.
    began_in: Option<web_sys::HtmlElement>,
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

/// The hidden editable element the browser types into, placed at the caret of whichever field holds the keyboard.
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
    composing: Rc<RefCell<Composing>>,
}

impl TextEntry {
    /// The entry `host` already holds, when an earlier reconcile left one there, or a new one appended after everything the host was served with.
    pub fn new(document: &web_sys::Document, host: &web_sys::HtmlElement) -> Option<Self> {
        let existing = host
            .query_selector(&format!(":scope > textarea[{ENTRY_ATTRIBUTE}]"))
            .ok()
            .flatten();
        let created = existing.is_none();
        let node = match existing {
            Some(node) => node,
            None => document.create_element("textarea").ok()?,
        }
        .dyn_into::<web_sys::HtmlElement>()
        .ok()?;
        let attributes = [
            ("style", Some(HIDDEN)),
            (ENTRY_ATTRIBUTE, Some("")),
            ("autocapitalize", Some("off")),
            ("autocorrect", Some("off")),
            ("spellcheck", Some("false")),
            // A single line as far as the browser is concerned; `Enter` is Telar's to interpret, and a textarea lets an input method compose without the element submitting anything.
            ("rows", Some("1")),
            // Deliberately not `aria-hidden`: this is where the keyboard actually is, and a focused element a reader has been told to ignore is worse than an unnamed one. It takes the name of whichever field it stands for in `park`.
            ("tabindex", Some("-1")),
            ("aria-label", Some(FALLBACK)),
            (CONSUMED_KEYS_ATTRIBUTE, None),
        ];
        for (name, value) in attributes {
            crate::adopt::patch_attribute(&node, name, value);
        }
        stand_by(&node, true);
        if created {
            host.append_child(node.as_ref()).ok()?;
        }

        let field: Standing = Rc::default();
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
                let (field, composing) = (field.clone(), composing.clone());
                move |event| on_input(event, &field, &composing)
            }),
            listen(target, "compositionstart", false, {
                let (field, composing) = (field.clone(), composing.clone());
                move |_| on_composition_start(&field, &composing)
            }),
            listen(target, "compositionupdate", false, {
                let (field, composing) = (field.clone(), composing.clone());
                move |event| on_composition_update(event, &field, &composing)
            }),
            listen(target, "compositionend", false, {
                let (field, composing) = (field.clone(), composing.clone());
                move |event| on_composition_end(event, &field, &composing)
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
            named: Some(FALLBACK.to_string()),
            active: false,
            keys: None,
            field,
            composing,
        })
    }

    /// Parks the entry at the caret of the field that holds the keyboard, and takes the keyboard with it.
    ///
    /// Focus goes here rather than to the field's own element, and that is the point: the element a person sees is not one a browser will type into, and the one it will type into must not be seen.
    ///
    /// It answers to the field's own name while it stands for it. A screen reader reads the *focused* element, which is this one and never the field, so an entry with no name is a person being told they are in an edit box and not which one — and an unnamed form control is a failure every accessibility audit reports. `FALLBACK` covers the field that named nothing, because "text field" read out is still better than silence.
    ///
    /// It also carries the keys the field keeps, since the platform reads them off whichever element the key was sent to. `take_focus` is false when a person moved focus elsewhere on the page: the entry still follows the field, and leaves the keyboard where it went.
    pub fn park(&mut self, field: Field<'_>, take_focus: bool) {
        let Field {
            rect,
            caret,
            multiline,
            label,
            keys,
            element,
        } = field;
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
        let name = label.filter(|label| !label.is_empty()).unwrap_or(FALLBACK);
        if self.named.as_deref() != Some(name) {
            let _ = self.node.set_attribute("aria-label", name);
            self.named = Some(name.to_string());
        }
        let _ = self
            .node
            .set_attribute("enterkeyhint", if multiline { "enter" } else { "done" });
        if self.keys.as_deref() != keys {
            match keys {
                Some(keys) => {
                    let _ = self.node.set_attribute(CONSUMED_KEYS_ATTRIBUTE, keys);
                }
                None => {
                    let _ = self.node.remove_attribute(CONSUMED_KEYS_ATTRIBUTE);
                }
            }
            self.keys = keys.map(str::to_string);
        }
        *self.field.borrow_mut() = element;
        // A press that left the keyboard in the field it was composing in did not end the composition.
        {
            let mut composing = self.composing.borrow_mut();
            if composing.active && composing.belongs_to(&self.field) {
                composing.pressed = false;
            }
        }
        self.active = true;
        stand_by(&self.node, false);
        if take_focus && !self.holds_focus() {
            let _ = self.node.focus();
        }
    }

    /// Takes the entry out of the way, for a frame where no field holds the keyboard.
    ///
    /// Only ever *releases*: blurring would be taking focus away from whatever just claimed it, and by the time this runs that is exactly what has happened.
    pub fn release(&mut self) {
        if !std::mem::take(&mut self.active) {
            return;
        }
        let _ = self.node.set_attribute("style", HIDDEN);
        self.placed.clear();
        *self.field.borrow_mut() = None;
        // Back to the generic name: standing for no field, it must not keep answering to the last one.
        if self.named.as_deref() != Some(FALLBACK) {
            let _ = self.node.set_attribute("aria-label", FALLBACK);
            self.named = Some(FALLBACK.to_string());
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

    fn holds_focus(&self) -> bool {
        is_focused(&self.node)
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
/// `beforeinput` rather than `keydown` because this is where the ones a key never named arrive: a deletion from a soft keyboard, a word replaced by autocorrect, a line dictated. The default is prevented in every case — the element must stay empty, or the next composition would start on top of the last one.
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
        // Everything else is an edit this field has no key for, a composition's own deletion among them. Prevented rather than let through, which would put text in an element nobody is going to read.
        _ => event.prevent_default(),
    }
}

/// The empty it has to go back to, and each step of a composition the browser wrote into it.
///
/// `beforeinput` prevents every edit, so outside a composition nothing lands here — except what an input method wrote directly, which no `beforeinput` precedes on every browser, and the text a composition ended with, which some browsers write again once it is over. Clearing unconditionally costs nothing and means a composition can never start on top of the last one.
fn on_input(event: &web_sys::Event, field: &Standing, composing: &RefCell<Composing>) {
    let Some(target) = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    else {
        return;
    };
    let in_composition = event
        .dyn_ref::<web_sys::InputEvent>()
        .is_some_and(web_sys::InputEvent::is_composing);
    if in_composition || composing.borrow().active {
        let selection = target
            .selection_start()
            .ok()
            .flatten()
            .zip(target.selection_end().ok().flatten());
        tell(
            field,
            composing,
            Ime::preedit_utf16(target.value(), selection),
        );
        return;
    }
    let value = target.value();
    if value.is_empty() {
        return;
    }
    let echo = composing.borrow_mut().committed.take();
    if echo.as_deref() != Some(value.as_str()) {
        post_text(&value);
    }
    target.set_value("");
}

fn on_composition_start(field: &Standing, composing: &RefCell<Composing>) {
    let mut composing = composing.borrow_mut();
    *composing = Composing {
        active: true,
        began_in: field.borrow().clone(),
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
fn on_composition_end(event: &web_sys::Event, field: &Standing, composing: &RefCell<Composing>) {
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
    if let Some(target) = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    {
        target.set_value("");
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
