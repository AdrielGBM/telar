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
    Space,
    Insert,
    CapsLock,
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
    PointerMoved {
        x: f64,
        y: f64,
        source: PointerSource,
    },
    // Note: pointer events intentionally omit ModifiersState. Detecting modifier chords (e.g. Shift+Click) requires tracking modifier state from KeyPressed / KeyReleased events externally. Add a `modifiers` field here if that becomes a first-class requirement.
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
    Scrolled {
        delta: ScrollDelta,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Auxiliary,
}
