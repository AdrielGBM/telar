#[derive(Debug, Clone)]
pub enum Event {
    WindowResized { width: u32, height: u32 },
    WindowCloseRequested,
    KeyPressed { key: String },
    KeyReleased { key: String },
    MouseMoved { x: f64, y: f64 },
    MousePressed { x: f64, y: f64, button: MouseButton },
    MouseReleased { x: f64, y: f64, button: MouseButton },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}
