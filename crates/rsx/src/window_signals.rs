use reactive_core::{ReadSignal, RwSignal, signal};

pub struct WindowSignals {
    pub width: ReadSignal<f32>,
    pub height: ReadSignal<f32>,
    pub(crate) width_rw: RwSignal<f32>,
    pub(crate) height_rw: RwSignal<f32>,
}

impl WindowSignals {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        let width_rw = signal(width);
        let height_rw = signal(height);
        Self {
            width: width_rw.read_only(),
            height: height_rw.read_only(),
            width_rw,
            height_rw,
        }
    }

    pub(crate) fn update(&self, width: f32, height: f32) {
        self.width_rw.set(width);
        self.height_rw.set(height);
    }
}
