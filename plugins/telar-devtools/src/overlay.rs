//! The overlay's widget tree: a window-sized [`SurfaceCanvas`] in the host's runtime, drawn over the app in the workbench theme.
//!
//! The canvas is a surface of its own, so its focus, overlays, direction and control size never mix with the app's, whether the app shares this runtime or lives in a hot-reloaded dylib with one of its own. Whoever holds the [`Overlay`] decides which events reach it: a pointer only over a panel, the keyboard only while a press on a panel left it focused there.

mod banner;
mod inspector;
mod stats;

use std::cell::Ref;

use telar::{
    BuildOutcome, Container, DevAction, Direction, DrawCommand, Event, EventResult, LayoutError,
    LayoutItem, LayoutStyle, OverlayResponse, RwSignal, Size, SizeDimension, SurfaceCanvas,
    box_item, focus, interactive_rects, signal,
};

use crate::inspect::{InspectSource, Inspection};
use crate::meter::Reading;
use crate::panels::mounted_while;
use crate::workbench::{WORKBENCH_CONTROL_SIZE, workbench_scope};

const DRAWER_WIDTH: f32 = 320.0;

/// Which panel the inspector drawer shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum DrawerTab {
    #[default]
    Components,
    Accessibility,
}

/// Everything the widgets read, as signals owned by the canvas: written from outside by [`Overlay`], and by the widgets themselves for what they own, such as the selection.
#[derive(Clone, Copy)]
pub(crate) struct Model {
    pub reading: RwSignal<Reading>,
    pub renderer: RwSignal<Option<String>>,
    pub build: RwSignal<BuildOutcome>,
    pub app_panic: RwSignal<Option<String>>,
    pub panel_open: RwSignal<bool>,
    pub inspector_open: RwSignal<bool>,
    pub drawer_tab: RwSignal<DrawerTab>,
    pub inspection: Inspection,
    pub drawer_width: RwSignal<f32>,
}

impl Model {
    fn new() -> Self {
        Self {
            reading: signal(Reading::default()),
            renderer: signal(None),
            build: signal(BuildOutcome::Loaded),
            app_panic: signal(None),
            panel_open: signal(false),
            inspector_open: signal(false),
            drawer_tab: signal(DrawerTab::default()),
            inspection: Inspection::new(),
            drawer_width: signal(DRAWER_WIDTH),
        }
    }
}

pub(crate) struct Overlay {
    canvas: SurfaceCanvas,
    model: Model,
    captured: bool,
    hovered: bool,
    keyboard: bool,
}

impl Overlay {
    pub fn new() -> Result<Self, LayoutError> {
        let mut built = None;
        let canvas = SurfaceCanvas::new(Size::ZERO, || {
            let model = Model::new();
            built = Some(model);
            Ok(box_item(workbench_scope(move || root(model))?))
        })?;
        canvas.set_direction(Some(Direction::Ltr));
        canvas.set_control_size(Some(WORKBENCH_CONTROL_SIZE));
        let model = built.expect("the canvas runs its build before it returns");
        Ok(Self {
            canvas,
            model,
            captured: false,
            hovered: false,
            keyboard: false,
        })
    }

    pub fn toggle_panel(&self) {
        let _entered = self.canvas.enter();
        self.model.panel_open.toggle();
    }

    pub fn toggle_inspector(&self) {
        let _entered = self.canvas.enter();
        self.model.inspector_open.toggle();
    }

    pub fn show_reading(&self, reading: Reading) {
        let _entered = self.canvas.enter();
        self.model.reading.set_if_changed(reading);
    }

    pub fn set_renderer(&self, renderer: &str) {
        let _entered = self.canvas.enter();
        self.model
            .renderer
            .set_if_changed(Some(renderer.to_owned()));
    }

    pub fn set_build_outcome(&self, outcome: BuildOutcome) {
        let _entered = self.canvas.enter();
        self.model.build.set_if_changed(outcome);
    }

    pub fn set_app_panic(&self, panic: Option<String>) {
        let _entered = self.canvas.enter();
        self.model.app_panic.set_if_changed(panic);
    }

    /// Reads what the runner reported of the app this frame, which only reaches the widgets where it differs from the last.
    pub fn refresh(&self, source: &dyn InspectSource) {
        let _entered = self.canvas.enter();
        self.model.inspection.refresh(source);
    }

    /// Whether an accessibility panel is open, which is what the runner reads the app's audit for.
    pub fn wants_audit(&self) -> bool {
        self.model.inspection.wants_audit()
    }

    /// Lays the canvas out at the window's logical size whenever that changes.
    pub fn fit(&self, size: Size) {
        if self.canvas.size() != size {
            self.canvas.resize(size);
        }
    }

    /// The overlay's frame in window coordinates, laid out against whatever changed since the last one.
    pub fn frame(&self) -> Ref<'_, Vec<DrawCommand>> {
        self.canvas.relayout_if_dirty();
        self.canvas.frame_commands()
    }

    /// The canvas's handle, which marks its frame inside the window's: its element ids name its own boxes, not the app's.
    pub fn surface(&self) -> u64 {
        self.canvas.surface().handle().0
    }

    pub fn is_dirty(&self) -> bool {
        self.canvas.relayout_if_dirty();
        self.canvas.is_dirty()
    }

    /// Hands `event` to the widgets if it is theirs, and says whether the app may still have it.
    pub fn route(&mut self, event: &Event) -> OverlayResponse {
        self.canvas.relayout_if_dirty();
        match event {
            Event::PointerPressed { x, y, .. } => self.press(event, *x as f32, *y as f32),
            Event::PointerMoved { x, y, .. } => self.moved(event, *x as f32, *y as f32),
            Event::PointerReleased { .. } if self.captured => {
                self.captured = false;
                self.dispatch(event);
                CONSUMED
            }
            Event::Scrolled { x, y, .. } | Event::ScrollEnded { x, y }
                if self.claims(*x as f32, *y as f32) =>
            {
                self.dispatch(event);
                CONSUMED
            }
            Event::CursorLeft if self.hovered => {
                self.hovered = false;
                self.dispatch(event);
                REDRAWN
            }
            Event::KeyPressed { .. } | Event::KeyReleased { .. } if self.holds_keyboard() => {
                match self.dispatch(event) {
                    EventResult::Handled => CONSUMED,
                    EventResult::Ignored => OverlayResponse::IGNORED,
                }
            }
            _ => OverlayResponse::IGNORED,
        }
    }

    fn press(&mut self, event: &Event, x: f32, y: f32) -> OverlayResponse {
        if !self.claims(x, y) {
            self.keyboard = false;
            let _entered = self.canvas.enter();
            if focus::current().is_none() {
                return OverlayResponse::IGNORED;
            }
            focus::clear();
            return REDRAWN;
        }
        self.captured = true;
        self.hovered = true;
        self.keyboard = true;
        {
            let _entered = self.canvas.enter();
            focus::blur_from_pointer(x, y);
        }
        self.dispatch(event);
        CONSUMED
    }

    fn moved(&mut self, event: &Event, x: f32, y: f32) -> OverlayResponse {
        let over = self.claims(x, y);
        let was_hovered = std::mem::replace(&mut self.hovered, over);
        if over || self.captured {
            self.dispatch(event);
            return CONSUMED;
        }
        if was_hovered {
            self.dispatch(&Event::CursorLeft);
            return REDRAWN;
        }
        OverlayResponse::IGNORED
    }

    /// Whether a point lands on something of the overlay's own: a panel, or a control on one. Everywhere else is the app's.
    fn claims(&self, x: f32, y: f32) -> bool {
        let _entered = self.canvas.enter();
        interactive_rects().iter().any(|rect| rect.contains(x, y))
    }

    fn holds_keyboard(&self) -> bool {
        let _entered = self.canvas.enter();
        self.keyboard && focus::current().is_some()
    }

    // Batched like the runner batches the tree's dispatch, so a handler's writes flush after every widget borrow is released.
    fn dispatch(&self, event: &Event) -> EventResult {
        telar::batch(|| self.canvas.dispatch(event))
    }
}

const CONSUMED: OverlayResponse = OverlayResponse {
    consumed: true,
    action: Some(DevAction::Redraw),
};

const REDRAWN: OverlayResponse = OverlayResponse {
    consumed: false,
    action: Some(DevAction::Redraw),
};

fn root(model: Model) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let layers = vec![
        mounted_while(
            LayoutStyle::new().absolute_fill(),
            move || model.inspector_open.get(),
            move || crate::panels::inspection_marks(model.inspection),
        )?,
        inspector::drawer(model)?,
        stats::corner(model)?,
        banner::banner(model)?,
    ];
    let root = Container::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        layers,
    )?;
    Ok(box_item(root))
}

#[cfg(test)]
#[path = "overlay_test.rs"]
mod tests;
