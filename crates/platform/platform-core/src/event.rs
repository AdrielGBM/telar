#[derive(Debug, Clone)]
pub enum Event {
    WindowResized {
        width: u32,
        height: u32,
    },
    WindowCloseRequested,
    KeyPressed {
        key: String,
    },
    KeyReleased {
        key: String,
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
