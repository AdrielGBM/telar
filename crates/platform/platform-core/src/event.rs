#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Enter,
    Backspace,
    Escape,
    Tab,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    Space,
    Insert,
    CapsLock,
    /// The numeric keypad, kept apart from the row of digits above the letters because an application that
    /// binds it (a modeller's orthographic views, a till, a calculator) means *that* key and not the digit.
    /// Only reported while Num Lock is on: with it off the OS says the key is `End`/`ArrowDown`/…, and
    /// overriding that would steal the arrows from someone navigating a list with the keypad.
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Named(NamedKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifiersState {
    pub is_shift: bool,
    pub is_ctrl: bool,
    pub is_alt: bool,
    pub is_meta: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScrollDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

impl ScrollDelta {
    /// This delta in pixels, whichever unit the platform reported it in.
    ///
    /// The conversion was written out at three call sites, each with the same bare `20.0` and a comment
    /// promising the copies agreed — which is a promise no comment can keep. A line is what a mouse wheel
    /// notch is worth, and it is one number for the whole runtime rather than a per-widget choice.
    pub fn pixels(&self) -> (f32, f32) {
        const PIXELS_PER_LINE: f32 = 20.0;
        match self {
            ScrollDelta::Lines { x, y } => (x * PIXELS_PER_LINE, y * PIXELS_PER_LINE),
            ScrollDelta::Pixels { x, y } => (*x, *y),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    WindowResized {
        width: u32,
        height: u32,
    },
    WindowCloseRequested,
    FocusChanged {
        is_focused: bool,
    },
    CursorEntered,
    CursorLeft,
    ScaleFactorChanged {
        scale_factor: f64,
    },
    KeyPressed {
        key: Key,
        modifiers: ModifiersState,
    },
    KeyReleased {
        key: Key,
        modifiers: ModifiersState,
    },
    /// The held modifier keys changed, and this is the authoritative reading.
    ///
    /// It has to be its own event because the key events cannot carry the news: a bare `Shift` press maps
    /// to no [`Key`] at all, so a chord that never types a character would be invisible. The platform also
    /// re-sends this when the window regains focus, which is what makes it safe to trust — a state
    /// reconstructed from presses and releases goes wrong exactly when the user alt-tabs mid-chord.
    ModifiersChanged {
        modifiers: ModifiersState,
    },
    PointerMoved {
        x: f64,
        y: f64,
        source: PointerSource,
    },
    // Pointer events carry no modifiers on purpose: a handler that needs to tell a click from a Shift-click reads the authoritative state (`ui_core::modifiers()`), which `ModifiersChanged` feeds. Widening these three signatures would have made every widget in the catalogue pay for a question two of them ask.
    PointerPressed {
        x: f64,
        y: f64,
        button: PointerButton,
        source: PointerSource,
    },
    PointerReleased {
        x: f64,
        y: f64,
        button: PointerButton,
        source: PointerSource,
    },
    /// A wheel turn or a scroll gesture, at the pointer position it happened over.
    ///
    /// The position is not in the OS event — neither winit's `MouseWheel` nor a touch drag carries one — so
    /// the platform layer fills it from the cursor it is already tracking. It belongs here rather than in
    /// each widget because every consumer needs it and every consumer would otherwise reconstruct it from
    /// the last move it happened to see: that is one guess per widget, all of them wrong for a wheel that
    /// arrives before the pointer has moved at all, and none of them able to say *zoom towards this point*.
    Scrolled {
        delta: ScrollDelta,
        x: f64,
        y: f64,
    },
    /// A box the *surface* scrolled, and where it stands now.
    ///
    /// The counterpart of [`Scrolled`](Self::Scrolled) rather than a variant of it, because it reports the
    /// opposite direction of causation. A wheel turn is an instruction: something asks to scroll and a widget
    /// decides what that means. This is a fact: it has already happened, the content is already drawn at the
    /// new offset, and the widget's own value is what needs correcting.
    ///
    /// It exists for a target that owns scrolling — a document, where the browser scrolls on the compositor
    /// and gives back find-in-page, `scrollIntoView`, anchors and the keyboard that a transform never could.
    /// The widget still holds the offset, because hit-testing, anchored overlays and `visible_rect` all read
    /// it; it just no longer decides it.
    ///
    /// `box_id` is opaque here: the layer that emits it and the layer that answers it agree on what it names,
    /// and nothing in between needs to know.
    BoxScrolled {
        box_id: u64,
        x: f32,
        y: f32,
    },
    // OS light/dark color-scheme preference changed (or was first reported at window creation). `dark` is
    // true for a dark preference. On Linux this is surfaced only when the compositor exposes it (Wayland +
    // xdg-desktop-portal); X11 sessions typically never emit it.
    ColorSchemeChanged {
        dark: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PointerSource {
    Mouse,
    Touch { id: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Auxiliary,
}
