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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModifiersState {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Debug, Clone)]
pub enum ScrollDelta {
    Lines { x: f32, y: f32 },
    Pixels { x: f32, y: f32 },
}

#[derive(Debug, Clone)]
pub enum Event {
    WindowResized {
        width: u32,
        height: u32,
    },
    WindowCloseRequested,
    FocusChanged {
        gained: bool,
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
