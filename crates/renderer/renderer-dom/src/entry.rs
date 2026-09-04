//! Typing, from an input method that will not talk to a canvas.
//!
//! Telar owns the text: the field is its widget, the caret is its own, and the value lives in a signal. What it does not own is *how characters are produced*. A browser gives that job to an editable element, and only to an editable element — a soft keyboard rises for one, an input method composes into one, a password manager fills one, dictation writes into one. A page that draws its fields itself gets none of that, and there is no key event to substitute for it: an IME reports `keydown` with a key that means "ask the input method", and the text arrives later, as a composition, addressed to a field.
//!
//! So there is one, parked over whichever field holds the keyboard and invisible on top of it. It never shows what was typed — Telar draws that — and it is emptied after every insertion, so it accumulates nothing and has no state to keep in step. What it produces goes back through the platform's own event queue as the keys the field would have received anyway, which is what lets the whole editing path — selection, undo, the caret — stay exactly where it was.

use platform_core::{Event, Key, ModifiersState, NamedKey};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;

use crate::paint;

/// Off the page, not `display:none`: a hidden element cannot be focused, and one with no size gives an input method nowhere to put its candidate window. It is over the field and invisible instead.
const HIDDEN: &str = "position:absolute;padding:0;border:0;outline:none;background:transparent;\
color:transparent;caret-color:transparent;font:inherit;resize:none;overflow:hidden;\
-webkit-user-select:text;user-select:text;";

/// What the entry answers to while standing for a field that named nothing.
const FALLBACK: &str = "Text field";

/// The hidden editable element the browser types into, placed over whichever field holds the keyboard.
pub struct TextEntry {
    node: web_sys::HtmlElement,
    /// Kept alive for as long as the entry: dropping a `Closure` unregisters the listener behind it.
    _listeners: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    /// Where it is parked, so an unchanged frame writes nothing.
    placed: String,
    /// The name it is currently answering to, for the same reason.
    named: Option<String>,
    /// Whether it currently stands for a field, so it can be taken out of the way when none does.
    active: bool,
}

impl TextEntry {
    pub fn new(document: &web_sys::Document, host: &web_sys::HtmlElement) -> Option<Self> {
        let node = document
            .create_element("textarea")
            .ok()?
            .dyn_into::<web_sys::HtmlElement>()
            .ok()?;
        let _ = node.set_attribute("style", HIDDEN);
        let _ = node.set_attribute("autocapitalize", "off");
        let _ = node.set_attribute("autocorrect", "off");
        let _ = node.set_attribute("spellcheck", "false");
        // A single line as far as the browser is concerned; `Enter` is Telar's to interpret, and a textarea lets an input method compose without the element submitting anything.
        let _ = node.set_attribute("rows", "1");
        // Deliberately not `aria-hidden`: this is where the keyboard actually is, and a focused element a reader has been told to ignore is worse than an unnamed one. It takes the name of whichever field it stands for in `park`.
        let _ = node.set_attribute("tabindex", "-1");
        // Named from the moment it exists, not only once a field has claimed it. The element is in the document for the whole life of the app and holds no field for most of it, and a control with no name is one an audit reports whenever it happens to look — which on a page nobody has typed into yet is always.
        let _ = node.set_attribute("aria-label", FALLBACK);
        host.append_child(node.as_ref()).ok()?;

        let listeners = vec![
            listen(&node, "keydown", on_key_down),
            listen(&node, "beforeinput", on_before_input),
            listen(&node, "input", on_input),
        ];
        Some(Self {
            node,
            _listeners: listeners,
            placed: String::new(),
            named: Some(FALLBACK.to_string()),
            active: false,
        })
    }

    /// Parks the entry over the field that holds the keyboard, and takes the keyboard with it.
    ///
    /// Focus goes here rather than to the field's own element, and that is the point: the element a person sees is not one a browser will type into, and the one it will type into must not be seen.
    ///
    /// It answers to the field's own name while it stands for it. A screen reader reads the *focused* element, which is this one and never the field, so an entry with no name is a person being told they are in an edit box and not which one — and an unnamed form control is a failure every accessibility audit reports. `FALLBACK` covers the field that named nothing, because "text field" read out is still better than silence.
    pub fn park(&mut self, rect: geometry_core::Rect, multiline: bool, label: Option<&str>) {
        let mut style = String::from(HIDDEN);
        paint::declare(&mut style, "left", &paint::px(rect.x));
        paint::declare(&mut style, "top", &paint::px(rect.y));
        paint::declare(&mut style, "width", &paint::px(rect.width.max(1.0)));
        paint::declare(&mut style, "height", &paint::px(rect.height.max(1.0)));
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
        self.active = true;
        if !self.holds_focus() {
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
        // Back to the generic name: standing for no field, it must not keep answering to the last one.
        if self.named.as_deref() != Some(FALLBACK) {
            let _ = self.node.set_attribute("aria-label", FALLBACK);
            self.named = Some(FALLBACK.to_string());
        }
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
        self.node
            .owner_document()
            .and_then(|document| document.active_element())
            .is_some_and(|active| active.is_same_node(Some(self.node.as_ref())))
    }
}

fn listen(
    node: &web_sys::HtmlElement,
    event: &str,
    handler: fn(&web_sys::Event),
) -> Closure<dyn FnMut(web_sys::Event)> {
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        handler(&event);
    });
    let _ = node.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
    closure
}

/// The edits a browser reports before making them, as the keys the field would have received.
///
/// `beforeinput` rather than `keydown` because this is where the ones a key never named arrive: a deletion from a soft keyboard, a word replaced by autocorrect, a line dictated. The default is prevented in every case — the element must stay empty, or the next composition would start on top of the last one.
fn on_before_input(event: &web_sys::Event) {
    let Some(event) = event.dyn_ref::<web_sys::InputEvent>() else {
        return;
    };
    // A composition in progress is not an edit yet: the input method is still deciding, and reports the result as one `insertText` when it is done.
    if event.is_composing() {
        return;
    }
    let kind = event.input_type();
    match kind.as_str() {
        "insertText" | "insertReplacementText" | "insertFromPaste" | "insertCompositionText" => {
            event.prevent_default();
            let Some(text) = event.data() else {
                return;
            };
            for character in text.chars() {
                post_key(Key::Char(character));
            }
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
        // Everything else is an edit this field has no key for. Prevented rather than let through, which would put text in an element nobody is going to read.
        _ => event.prevent_default(),
    }
}

/// The empty it has to go back to.
///
/// `beforeinput` prevents every edit, so in principle nothing lands here — except what an input method wrote directly, which no `beforeinput` precedes on every browser. Clearing unconditionally costs nothing and means a composition can never start on top of the last one.
fn on_input(event: &web_sys::Event) {
    let Some(target) = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
    else {
        return;
    };
    let value = target.value();
    if value.is_empty() {
        return;
    }
    for character in value.chars() {
        post_key(Key::Char(character));
    }
    target.set_value("");
}

/// One key, pressed and released, as the field would have seen it.
///
/// Both halves because a widget is entitled to expect them in pairs: one that tracked a held key would otherwise believe every character typed here is still down.
fn post_key(key: Key) {
    let modifiers = ModifiersState::default();
    platform_core::post_event(Event::KeyPressed {
        key: key.clone(),
        modifiers,
    });
    platform_core::post_event(Event::KeyReleased { key, modifiers });
}

/// Keeps a character from being typed twice.
///
/// While the entry holds the keyboard there are two ways for one keystroke to become text: the platform's own `keydown` listener on the host, and the `beforeinput` this element reports. Both fire, and the field received every letter twice.
///
/// The split is by what the two can each see. A printable character is exactly what `beforeinput` describes better — it is the same event whether it came from a key, a soft keyboard, an input method or dictation — so it stops here. Everything else, arrows and Escape and every shortcut, produces no input event at all, and goes on up to the platform as it always did.
fn on_key_down(event: &web_sys::Event) {
    let Some(event) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
        return;
    };
    // The keys an input method is in the middle of consuming are its own, whatever they say they are.
    if event.is_composing() {
        event.stop_propagation();
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
