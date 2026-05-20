use reactive_core::{ReadSignal, RwSignal, create_rw_signal};

pub struct WindowSignals {
    pub width: ReadSignal<f32>,
    pub height: ReadSignal<f32>,
    pub(crate) width_w: RwSignal<f32>,
    pub(crate) height_w: RwSignal<f32>,
}

impl WindowSignals {
    pub fn new(width: f32, height: f32) -> Self {
        let width_w = create_rw_signal(width);
        let height_w = create_rw_signal(height);
        Self {
            width: width_w.read_only(),
            height: height_w.read_only(),
            width_w,
            height_w,
        }
    }

    pub fn update(&self, width: f32, height: f32) {
        self.width_w.set(width);
        self.height_w.set(height);
    }
}
