//! The overlay's widget tree: a window-sized [`SurfaceCanvas`] in the host's runtime, drawn over the app in the workbench theme.
//!
//! The canvas is a surface of its own, so its focus, overlays, direction and control size never mix with the app's, whether the app shares this runtime or lives in a hot-reloaded dylib with one of its own. Whoever holds the [`Overlay`] decides which events reach it: a pointer only over a panel, the keyboard only while a press on a panel left it focused there.

mod banner;
mod inspector;
mod stats;

use std::cell::Ref;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    Container, DevAction, Direction, DrawCommand, Event, EventResult, LayoutError, LayoutItem,
    LayoutStyle, OverlayResponse, ReactiveList, RwSignal, SegmentNodeInfo, Size, SizeDimension,
    SurfaceCanvas, box_item, focus, interactive_rects, signal,
};

use crate::meter::Reading;
use crate::workbench::{WORKBENCH_CONTROL_SIZE, workbench_scope};

const DRAWER_WIDTH: f32 = 320.0;
const DETAILS_HEIGHT: f32 = 220.0;

/// Everything the widgets read, as signals owned by the canvas: written from outside by [`Overlay`], and by the widgets themselves for what they own, such as the selection.
#[derive(Clone, Copy)]
pub(crate) struct Model {
    pub reading: RwSignal<Reading>,
    pub renderer: RwSignal<Option<String>>,
    pub build_error: RwSignal<Option<String>>,
    pub panel_open: RwSignal<bool>,
    pub inspector_open: RwSignal<bool>,
    pub nodes: RwSignal<Rc<[SegmentNodeInfo]>>,
    pub selected: RwSignal<Option<Arc<str>>>,
    pub expanded: RwSignal<HashSet<Arc<str>>>,
    pub drawer_width: RwSignal<f32>,
    pub details_height: RwSignal<f32>,
}

impl Model {
    fn new() -> Self {
        Self {
            reading: signal(Reading::default()),
            renderer: signal(None),
            build_error: signal(None),
            panel_open: signal(false),
            inspector_open: signal(false),
            nodes: signal(Rc::from([])),
            selected: signal(None),
            expanded: signal(HashSet::new()),
            drawer_width: signal(DRAWER_WIDTH),
            details_height: signal(DETAILS_HEIGHT),
        }
    }

    /// The node the inspector has selected, as the last walk reported it.
    pub fn selected_node(&self) -> Option<SegmentNodeInfo> {
        let id: u64 = self.selected.get()?.parse().ok()?;
        self.nodes
            .with(|nodes| nodes.iter().find(|node| node.id == id).cloned())
    }
}

pub(crate) struct Overlay {
    canvas: SurfaceCanvas,
    model: Model,
    seen_branches: HashSet<u64>,
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
            seen_branches: HashSet::new(),
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

    pub fn set_build_error(&self, error: Option<String>) {
        let _entered = self.canvas.enter();
        self.model.build_error.set_if_changed(error);
    }

    /// Takes the walk the runner made of the app this frame, which only reaches the widgets when it differs from the last one.
    pub fn set_nodes(&mut self, nodes: &[SegmentNodeInfo]) {
        if self
            .model
            .nodes
            .peek_with(|current| same_walk(current, nodes))
        {
            return;
        }
        let _entered = self.canvas.enter();
        let opened: Vec<Arc<str>> = branches(nodes)
            .filter(|id| self.seen_branches.insert(*id))
            .map(|id| Arc::from(id.to_string()))
            .collect();
        if !opened.is_empty() {
            self.model.expanded.update(|open| open.extend(opened));
        }
        self.model.nodes.set(Rc::from(nodes));
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
        inspector::highlight(model)?,
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

/// A layer mounted only while `shown` says so, positioned by `style`.
fn mounted_while(
    style: LayoutStyle,
    shown: impl Fn() -> bool + 'static,
    build: impl Fn() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let layer = ReactiveList::with_style(
        style,
        move || if shown() { vec![()] } else { Vec::new() },
        |_: &()| (),
        move |()| build(),
    )?;
    Ok(box_item(layer))
}

fn same_walk(a: &[SegmentNodeInfo], b: &[SegmentNodeInfo]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.id == b.id
                && a.name == b.name
                && a.depth == b.depth
                && a.rect == b.rect
                && a.padding == b.padding
                && a.margin == b.margin
                && a.border == b.border
                && a.gap == b.gap
        })
}

/// The ids of the nodes in a pre-order walk that have nodes under them.
fn branches(nodes: &[SegmentNodeInfo]) -> impl Iterator<Item = u64> + '_ {
    nodes
        .windows(2)
        .filter(|pair| pair[1].depth > pair[0].depth)
        .map(|pair| pair[0].id)
}
