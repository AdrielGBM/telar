use std::cell::{Cell, RefCell};
use std::rc::Rc;

use geometry_core::{Insets, Rect, Size};
use i18n_core::{Catalog, Entry, Message};
use layout_core::{AvailableSpace, Direction, LayoutError, LayoutStyle, SizeDimension};
use platform_core::{Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource};
use reactive_core::{RwSignal, effect, signal};
use renderer_core::{Color, DrawCommand, RectStyle, ShapeStyle};
use theme_core::ControlSize;
use ui_tree::ComponentList;

use super::*;
use crate::dismiss::{self, DismissRegistration};
use crate::{
    Container, Overlay, StyledContainer, WindowRoot, compute_layout, relayout_if_dirty,
    reset_layout_runtime, use_safe_area_insets, use_surface_size,
};

const WINDOW: f32 = 400.0;
const FRAME: Rect = Rect {
    x: 30.0,
    y: 50.0,
    width: 200.0,
    height: 100.0,
};

fn fill() -> LayoutStyle {
    LayoutStyle::new()
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
}

fn boxed(item: impl LayoutItem + 'static) -> Box<dyn LayoutItem> {
    Box::new(item)
}

fn press(x: f32, y: f32) -> Event {
    Event::PointerPressed {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn release(x: f32, y: f32) -> Event {
    Event::PointerReleased {
        x: x as f64,
        y: y as f64,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

fn key(key: NamedKey, shift: bool) -> Event {
    Event::KeyPressed {
        key: Key::Named(key),
        modifiers: ModifiersState {
            is_shift: shift,
            ..ModifiersState::default()
        },
    }
}

fn swatch(color: Color, style: LayoutStyle) -> Result<StyledContainer, LayoutError> {
    StyledContainer::new(
        style,
        move |_| RectStyle::default().with_fill(color),
        vec![],
    )
}

fn focusable(color: Color, focused: RwSignal<bool>) -> Box<dyn LayoutItem> {
    boxed(
        swatch(color, LayoutStyle::new().width(20.0).height(20.0))
            .unwrap()
            .on_focus(move |now| focused.set(now)),
    )
}

/// A window laid out the way a runner lays one out, with `frame` at [`FRAME`] and `before` and `after` beside it in the tab order.
struct Window {
    tree: ComponentList,
}

impl Window {
    fn new(
        frame: SurfaceFrame,
        before: Box<dyn LayoutItem>,
        after: Box<dyn LayoutItem>,
        on_press: impl Fn() + 'static,
    ) -> Self {
        let spacer = |width: f32, height: f32| {
            boxed(Container::new(LayoutStyle::new().width(width).height(height), vec![]).unwrap())
        };
        let row = Container::new(
            LayoutStyle::new().flex_row().height(FRAME.height),
            vec![spacer(FRAME.x, FRAME.height), boxed(frame)],
        )
        .unwrap();
        let content = Container::new(
            fill().flex_column(),
            vec![spacer(10.0, FRAME.y), boxed(row), before, after],
        )
        .unwrap()
        .on_press(on_press);
        let root = content.layout_node();
        crate::set_surface_size(Size::new(WINDOW, WINDOW));
        let tree = ComponentList::new(WindowRoot::new(boxed(content)));
        compute_layout(
            root,
            AvailableSpace::Definite(WINDOW),
            AvailableSpace::Definite(WINDOW),
        )
        .unwrap();
        let window = Self { tree };
        window.frame();
        window
    }

    /// One redraw: the runner lays out what changed, which is when every frame in the window keeps up, then composes.
    fn frame(&self) -> Vec<DrawCommand> {
        relayout_if_dirty();
        self.tree.commands().clone()
    }

    /// Routes an event as the runner does: the window's own overlay layer first, then the tree.
    fn send(&self, event: Event) {
        if crate::dispatch_overlays(&event) == EventResult::Ignored {
            self.tree.dispatch(&event);
        }
    }

    fn click(&self, x: f32, y: f32) {
        self.send(press(x, y));
        self.send(release(x, y));
    }
}

fn no_focusable() -> Box<dyn LayoutItem> {
    boxed(Container::new(LayoutStyle::new(), vec![]).unwrap())
}

/// Each fill drawn in the frame, with the rect it lands on in the window and the clip it is drawn under.
fn fills(commands: &[DrawCommand], color: Color) -> Vec<(Rect, Option<Rect>)> {
    let mut clips: Vec<Rect> = Vec::new();
    let mut found = Vec::new();
    renderer_core::for_each_with_matrix(commands, |command, matrix| match command {
        DrawCommand::PushClip { rect, .. } => {
            clips.push(renderer_core::transform_clip_rect(matrix, *rect))
        }
        DrawCommand::PopClip => {
            clips.pop();
        }
        DrawCommand::Rect { rect, style, .. } if style.fill == Some(color.into()) => found.push((
            renderer_core::transform_clip_rect(matrix, *rect),
            clips.last().copied(),
        )),
        _ => {}
    });
    found
}

struct Modal {
    open: RwSignal<bool>,
    hits: Rc<Cell<u32>>,
    behind: Rc<Cell<u32>>,
}

/// A page with a modal over it, which registers on the dismiss stack while open the way a dialog component does.
fn page_with_modal(modal: &Modal) -> impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (open, hits, behind) = (modal.open, modal.hits.clone(), modal.behind.clone());
    move || {
        let page = swatch(Color::WHITE, fill())?.on_press(move || behind.set(behind.get() + 1));
        let scrim = swatch(Color::BLACK, fill())?.on_press(move || hits.set(hits.get() + 1));
        let overlay =
            Overlay::toggleable(LayoutStyle::new(), vec![boxed(scrim)], move || open.get())?;
        let held: RefCell<Option<DismissRegistration>> = RefCell::new(None);
        effect(move || {
            if open.get() {
                held.borrow_mut().get_or_insert_with(|| {
                    DismissRegistration::new(Rc::new(move || open.set(false)))
                });
            } else {
                held.borrow_mut().take();
            }
        });
        Ok(boxed(Container::new(
            fill(),
            vec![boxed(page), boxed(overlay)],
        )?))
    }
}

fn modal() -> Modal {
    Modal {
        open: signal(false),
        hits: Rc::new(Cell::new(0)),
        behind: Rc::new(Cell::new(0)),
    }
}

#[test]
fn a_modal_opened_inside_a_frame_stays_inside_its_bounds_and_its_surface() {
    reset_layout_runtime();
    let modal = modal();
    let canvas = Rc::new(SurfaceCanvas::new(Size::new(1.0, 1.0), page_with_modal(&modal)).unwrap());
    let window_hits = Rc::new(Cell::new(0));
    let window = Window::new(
        SurfaceFrame::filling(
            Rc::clone(&canvas),
            LayoutStyle::new().width(FRAME.width).height(FRAME.height),
        )
        .unwrap(),
        no_focusable(),
        no_focusable(),
        {
            let window_hits = window_hits.clone();
            move || window_hits.set(window_hits.get() + 1)
        },
    );

    modal.open.set(true);
    let drawn = fills(&window.frame(), Color::BLACK);
    assert_eq!(
        drawn,
        [(FRAME, Some(FRAME))],
        "the scrim fills the frame, not the window, and is drawn under the frame's clip"
    );

    assert_eq!(
        dismiss::dismiss_depth(),
        0,
        "the window's dismiss stack knows nothing of it"
    );
    {
        let _inside = canvas.enter();
        assert_eq!(
            dismiss::dismiss_depth(),
            1,
            "the frame's surface holds the modal"
        );
    }
    assert_eq!(
        crate::dispatch_overlays(&press(100.0, 100.0)),
        EventResult::Ignored,
        "the window's overlay layer has no modal to route to"
    );

    window.click(100.0, 100.0);
    assert_eq!(
        modal.hits.get(),
        1,
        "a press inside the frame reaches the modal"
    );
    assert_eq!(modal.behind.get(), 0, "and not the page behind it");
    assert_eq!(window_hits.get(), 0, "nor the window around the frame");

    window.click(300.0, 300.0);
    assert_eq!(
        window_hits.get(),
        1,
        "the modal blocks nothing outside the frame"
    );
    assert_eq!(modal.hits.get(), 1);

    window.send(key(NamedKey::Escape, false));
    assert!(
        modal.open.get(),
        "Escape pressed while the window holds the keyboard is not the frame's"
    );

    window.click(100.0, 100.0);
    window.send(key(NamedKey::Escape, false));
    assert!(
        !modal.open.get(),
        "Escape pressed while the frame holds the keyboard closes its modal"
    );
    assert!(fills(&window.frame(), Color::BLACK).is_empty());
}

#[test]
fn pointer_events_reach_the_frame_in_its_own_coordinates_and_scale() {
    reset_layout_runtime();
    let pressed = signal(0u32);
    let moved_to = Rc::new(Cell::new(None));
    let canvas = {
        let moved_to = moved_to.clone();
        Rc::new(
            SurfaceCanvas::new(Size::new(50.0, 40.0), move || {
                let target = swatch(Color::BLACK, LayoutStyle::new().width(20.0).height(20.0))?
                    .on_press(move || pressed.set(pressed.get() + 1));
                Ok(boxed(Probe {
                    inner: boxed(Container::new(
                        fill().padding_all(10.0),
                        vec![boxed(target)],
                    )?),
                    moved_to,
                }))
            })
            .unwrap(),
        )
    };
    canvas.set_scale(2.0);
    let window = Window::new(
        SurfaceFrame::sized(Rc::clone(&canvas), LayoutStyle::new()).unwrap(),
        no_focusable(),
        no_focusable(),
        || {},
    );
    window.frame();

    assert_eq!(
        fills(&window.frame(), Color::BLACK),
        [(
            Rect::new(50.0, 70.0, 40.0, 40.0),
            Some(Rect::new(30.0, 50.0, 100.0, 80.0))
        )],
        "a 20×20 box at (10, 10) in a 50×40 surface shown at 2× from (30, 50)"
    );

    window.click(45.0, 60.0);
    assert_eq!(
        pressed.get(),
        0,
        "(45, 60) is the surface's (7.5, 5), beside the box"
    );
    window.click(60.0, 80.0);
    assert_eq!(
        pressed.get(),
        1,
        "(60, 80) is the surface's (15, 15), on it"
    );

    window.send(Event::PointerMoved {
        x: 70.0,
        y: 90.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(moved_to.get(), Some((20.0, 20.0)));
    window.send(Event::PointerMoved {
        x: 300.0,
        y: 300.0,
        source: PointerSource::Mouse,
    });
    assert_eq!(
        moved_to.get(),
        None,
        "a pointer leaving the frame reaches it as leaving, not as a move it would hit-test"
    );
}

/// Records where pointer moves land inside the surface, and that the pointer left it.
struct Probe {
    inner: Box<dyn LayoutItem>,
    moved_to: Rc<Cell<Option<(f64, f64)>>>,
}

impl Component for Probe {
    fn view(&self) -> RenderNode {
        self.inner.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::PointerMoved { x, y, .. } => self.moved_to.set(Some((*x, *y))),
            Event::CursorLeft => self.moved_to.set(None),
            _ => {}
        }
        self.inner.on_event(event)
    }
}

impl LayoutItem for Probe {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }
}

/// Records where a scroll gesture ended inside the surface.
struct ScrollEndProbe {
    inner: Box<dyn LayoutItem>,
    ended_at: Rc<Cell<Option<(f64, f64)>>>,
}

impl Component for ScrollEndProbe {
    fn view(&self) -> RenderNode {
        self.inner.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if let Event::ScrollEnded { x, y } = event {
            self.ended_at.set(Some((*x, *y)));
        }
        self.inner.on_event(event)
    }
}

impl LayoutItem for ScrollEndProbe {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }
}

#[test]
fn an_overlay_inside_a_frame_hears_a_scroll_end_in_the_frames_coordinates() {
    reset_layout_runtime();
    let ended_at = Rc::new(Cell::new(None));
    let canvas = {
        let ended_at = ended_at.clone();
        Rc::new(
            SurfaceCanvas::new(Size::new(50.0, 40.0), move || {
                let panel = ScrollEndProbe {
                    inner: boxed(swatch(Color::BLACK, fill())?),
                    ended_at,
                };
                let overlay = Overlay::toggleable(LayoutStyle::new(), vec![boxed(panel)], || true)?;
                Ok(boxed(Container::new(fill(), vec![boxed(overlay)])?))
            })
            .unwrap(),
        )
    };
    canvas.set_scale(2.0);
    let window = Window::new(
        SurfaceFrame::sized(Rc::clone(&canvas), LayoutStyle::new()).unwrap(),
        no_focusable(),
        no_focusable(),
        || {},
    );

    window.send(Event::ScrollEnded { x: 70.0, y: 90.0 });
    assert_eq!(
        ended_at.get(),
        Some((20.0, 20.0)),
        "(70, 90) is the surface's (20, 20), mapped as a wheel turn there would be"
    );

    ended_at.set(None);
    window.send(Event::ScrollEnded { x: 300.0, y: 300.0 });
    assert_eq!(
        ended_at.get(),
        None,
        "a gesture ending outside the frame is not its own"
    );
}

#[test]
fn focus_inside_a_frame_does_not_escape_it() {
    reset_layout_runtime();
    let [outer_before, outer_after, first, second] = [(); 4].map(|_| signal(false));
    let canvas = Rc::new(
        SurfaceCanvas::new(Size::new(1.0, 1.0), move || {
            Ok(boxed(Container::new(
                fill().flex_row(),
                vec![
                    focusable(Color::BLACK, first),
                    focusable(Color::WHITE, second),
                ],
            )?))
        })
        .unwrap(),
    );
    let window = Window::new(
        SurfaceFrame::filling(
            Rc::clone(&canvas),
            LayoutStyle::new().width(FRAME.width).height(FRAME.height),
        )
        .unwrap(),
        focusable(Color::BLACK, outer_before),
        focusable(Color::WHITE, outer_after),
        || {},
    );

    window.click(35.0, 55.0);
    assert!(
        first.get(),
        "a press on the first box inside takes focus there"
    );

    for expected in [second, first, second, first] {
        window.send(key(NamedKey::Tab, false));
        assert!(
            expected.get(),
            "Tab walks the focusables inside the frame and wraps there"
        );
        assert!(
            !outer_before.get() && !outer_after.get(),
            "and never reaches the window's"
        );
    }
    window.send(key(NamedKey::Tab, true));
    assert!(
        second.get(),
        "Shift+Tab wraps backwards inside the frame too"
    );

    window.click(5.0, 165.0);
    assert!(
        outer_before.get(),
        "a press on the window's own box takes focus there"
    );
    assert!(
        !first.get() && !second.get(),
        "leaving the frame clears focus inside it"
    );

    window.send(key(NamedKey::Tab, false));
    window.send(key(NamedKey::Tab, false));
    assert!(
        first.get(),
        "Tab arriving at the frame hands the keyboard to the first focusable inside"
    );
}

#[test]
fn the_window_hears_that_a_field_inside_the_frame_has_the_caret() {
    reset_layout_runtime();
    let canvas = Rc::new(
        SurfaceCanvas::new(Size::new(1.0, 1.0), || {
            Ok(boxed(Container::new(fill(), vec![])?))
        })
        .unwrap(),
    );
    let window = Window::new(
        SurfaceFrame::filling(
            Rc::clone(&canvas),
            LayoutStyle::new().width(FRAME.width).height(FRAME.height),
        )
        .unwrap(),
        no_focusable(),
        no_focusable(),
        || {},
    );
    let field = {
        let _inside = canvas.enter();
        let field = focus::next_id();
        focus::register_as(field, FocusKind::TextEntry);
        field
    };

    window.click(100.0, 100.0);
    assert!(!focus::text_entry_focused());
    {
        let _inside = canvas.enter();
        focus::request(field);
    }
    assert!(
        focus::text_entry_focused(),
        "a shortcut table in the window must stand aside while the field inside has the caret"
    );
}

#[test]
fn size_and_safe_area_reach_readers_inside_the_frame() {
    reset_layout_runtime();
    let size_read = Rc::new(Cell::new(Size::new(0.0, 0.0)));
    let insets_read = Rc::new(Cell::new(Insets::default()));
    let canvas = {
        let (size_read, insets_read) = (size_read.clone(), insets_read.clone());
        Rc::new(
            SurfaceCanvas::new(Size::new(1.0, 1.0), move || {
                effect(move || size_read.set(use_surface_size()));
                effect(move || insets_read.set(use_safe_area_insets()));
                Ok(boxed(Container::new(fill(), vec![])?))
            })
            .unwrap(),
        )
    };
    canvas.set_scale(0.5);
    let window = Window::new(
        SurfaceFrame::filling(
            Rc::clone(&canvas),
            LayoutStyle::new().width(FRAME.width).height(FRAME.height),
        )
        .unwrap(),
        no_focusable(),
        no_focusable(),
        || {},
    );
    window.frame();
    assert_eq!(
        size_read.get(),
        Size::new(400.0, 200.0),
        "a 200×100 frame at half scale shows a 400×200 surface"
    );
    assert_eq!(
        crate::surface_size(),
        Size::new(WINDOW, WINDOW),
        "the window keeps its own size"
    );

    let notch = Insets::new(44.0, 0.0, 34.0, 0.0);
    canvas.set_safe_area(notch);
    assert_eq!(insets_read.get(), notch);
    assert_eq!(
        use_safe_area_insets(),
        Insets::default(),
        "the window's own safe area is untouched"
    );
}

#[test]
fn a_sized_frame_takes_the_box_of_its_surface_at_its_scale() {
    reset_layout_runtime();
    let canvas = Rc::new(
        SurfaceCanvas::new(Size::new(400.0, 800.0), || {
            Ok(boxed(Container::new(fill(), vec![])?))
        })
        .unwrap(),
    );
    canvas.set_scale(0.25);
    let frame = SurfaceFrame::sized(Rc::clone(&canvas), LayoutStyle::new()).unwrap();
    let rect = frame.leaf.rect;
    let window = Window::new(frame, no_focusable(), no_focusable(), || {});
    assert_eq!((rect.get().width, rect.get().height), (100.0, 200.0));

    canvas.resize(Size::new(800.0, 400.0));
    window.frame();
    assert_eq!(
        (rect.get().width, rect.get().height),
        (200.0, 100.0),
        "turning the device turns the frame"
    );
}

const LEAD: Color = Color::rgba(0.9, 0.2, 0.2, 1.0);
const LEAD_WIDTH: f32 = 40.0;

/// A surface whose first box starts the row, wherever the direction in force says a row starts, and which records the direction its effects read.
fn leading_swatch(read: Rc<RefCell<Vec<Direction>>>) -> SurfaceCanvas {
    SurfaceCanvas::new(Size::new(1.0, 1.0), move || {
        effect(move || read.borrow_mut().push(crate::use_direction()));
        let lead = swatch(LEAD, LayoutStyle::new().width(LEAD_WIDTH).height(20.0))?;
        Ok(boxed(Container::new(fill().flex_row(), vec![boxed(lead)])?))
    })
    .unwrap()
}

fn lead_x(window: &Window) -> f32 {
    let drawn = fills(&window.frame(), LEAD);
    assert_eq!(drawn.len(), 1, "the frame draws its leading box once");
    drawn[0].0.x
}

#[test]
fn a_direction_set_on_a_frame_lays_out_inside_it_and_never_reaches_the_window() {
    reset_layout_runtime();
    crate::set_direction(Direction::Ltr);
    let read = Rc::new(RefCell::new(Vec::new()));
    let canvas = Rc::new(leading_swatch(read.clone()));
    let frame = SurfaceFrame::filling(
        Rc::clone(&canvas),
        LayoutStyle::new().width(FRAME.width).height(FRAME.height),
    )
    .unwrap();
    let placed = frame.leaf.rect;
    let window = Window::new(frame, no_focusable(), no_focusable(), || {});
    assert_eq!(lead_x(&window), FRAME.x);

    canvas.set_direction(Some(Direction::Rtl));
    assert_eq!(canvas.direction(), Some(Direction::Rtl));
    assert_eq!(
        lead_x(&window),
        FRAME.x + FRAME.width - LEAD_WIDTH,
        "the row inside the frame starts at its right edge"
    );
    assert_eq!(
        placed.get().x,
        FRAME.x,
        "the window still lays out left to right"
    );
    assert_eq!(crate::current_direction(), Direction::Ltr);
    assert_eq!(read.borrow().last(), Some(&Direction::Rtl));

    canvas.set_direction(None);
    assert_eq!(
        lead_x(&window),
        FRAME.x,
        "cleared, the frame follows the window's direction again"
    );
    assert_eq!(read.borrow().last(), Some(&Direction::Ltr));
}

#[test]
fn a_frame_kept_left_to_right_stays_so_inside_a_right_to_left_window() {
    reset_layout_runtime();
    crate::set_direction(Direction::Rtl);
    let read = Rc::new(RefCell::new(Vec::new()));
    let canvas = Rc::new(leading_swatch(read.clone()));
    canvas.set_direction(Some(Direction::Ltr));
    let frame = SurfaceFrame::filling(
        Rc::clone(&canvas),
        LayoutStyle::new().width(FRAME.width).height(FRAME.height),
    )
    .unwrap();
    let placed = frame.leaf.rect;
    let window = Window::new(frame, no_focusable(), no_focusable(), || {});
    let mirrored_x = WINDOW - FRAME.x - FRAME.width;
    assert_eq!(
        placed.get().x,
        mirrored_x,
        "the window lays out right to left"
    );
    assert_eq!(lead_x(&window), mirrored_x, "and the frame left to right");
    assert_eq!(read.borrow().last(), Some(&Direction::Ltr));
    crate::set_direction(Direction::Ltr);
}

#[test]
fn a_frame_without_a_direction_of_its_own_follows_the_window() {
    reset_layout_runtime();
    crate::set_direction(Direction::Ltr);
    let read = Rc::new(RefCell::new(Vec::new()));
    let canvas = Rc::new(leading_swatch(read.clone()));
    let frame = SurfaceFrame::filling(
        Rc::clone(&canvas),
        LayoutStyle::new().width(FRAME.width).height(FRAME.height),
    )
    .unwrap();
    let placed = frame.leaf.rect;
    let window = Window::new(frame, no_focusable(), no_focusable(), || {});
    assert_eq!(lead_x(&window), FRAME.x);

    crate::set_direction(Direction::Rtl);
    let mirrored_x = WINDOW - FRAME.x - FRAME.width;
    assert_eq!(lead_x(&window), mirrored_x + FRAME.width - LEAD_WIDTH);
    assert_eq!(placed.get().x, mirrored_x);
    assert_eq!(*read.borrow(), vec![Direction::Ltr, Direction::Rtl]);
    crate::set_direction(Direction::Ltr);
}

static CATALOG: Catalog = Catalog {
    locales: &["en", "es"],
    default_locale: "en",
    entries: &[
        Entry {
            key: "farewell",
            messages: &[
                ("en", Message::Plain("Bye")),
                ("es", Message::Plain("Adiós")),
            ],
        },
        Entry {
            key: "greeting",
            messages: &[
                ("en", Message::Plain("Hello")),
                ("es", Message::Plain("Hola")),
            ],
        },
    ],
};

fn translated(key: &'static str) -> Box<dyn LayoutItem> {
    boxed(
        crate::Text::declaring(
            move || i18n_core::translate(&CATALOG, key, &[]),
            LayoutStyle::new(),
            |text| text,
        )
        .unwrap(),
    )
}

fn texts(commands: &[DrawCommand]) -> Vec<String> {
    let mut found = Vec::new();
    renderer_core::for_each_with_matrix(commands, |command, _| {
        if let DrawCommand::Text { text, .. } = command {
            found.push(text.to_string());
        }
    });
    found.sort();
    found
}

#[test]
fn a_locale_set_on_a_frame_translates_the_text_inside_it_only() {
    reset_layout_runtime();
    i18n_core::set_locale("en");
    let canvas = Rc::new(
        SurfaceCanvas::new(Size::new(1.0, 1.0), || {
            Ok(boxed(Container::new(fill(), vec![translated("greeting")])?))
        })
        .unwrap(),
    );
    let window = Window::new(
        SurfaceFrame::filling(
            Rc::clone(&canvas),
            LayoutStyle::new().width(FRAME.width).height(FRAME.height),
        )
        .unwrap(),
        translated("farewell"),
        no_focusable(),
        || {},
    );
    assert_eq!(texts(&window.frame()), ["Bye", "Hello"]);

    canvas.set_locale(Some("es"));
    assert_eq!(canvas.locale().as_deref(), Some("es"));
    assert_eq!(
        texts(&window.frame()),
        ["Bye", "Hola"],
        "the frame speaks Spanish and the window around it does not"
    );
    assert_eq!(i18n_core::current_locale().as_deref(), Some("en"));

    canvas.set_locale(None);
    assert_eq!(
        texts(&window.frame()),
        ["Bye", "Hello"],
        "cleared, the frame follows the window's locale again"
    );

    i18n_core::set_locale("es");
    assert_eq!(
        texts(&window.frame()),
        ["Adiós", "Hola"],
        "a frame with no locale of its own hears the window's change"
    );
    i18n_core::set_locale("en");
}

#[test]
fn a_control_size_set_on_a_frame_reaches_only_the_readers_inside_it() {
    reset_layout_runtime();
    theme_core::set_control_size(ControlSize::Regular);
    let inside = Rc::new(RefCell::new(Vec::new()));
    let outside = Rc::new(RefCell::new(Vec::new()));
    let canvas = {
        let inside = inside.clone();
        Rc::new(
            SurfaceCanvas::new(Size::new(1.0, 1.0), move || {
                effect(move || inside.borrow_mut().push(theme_core::use_control_size()));
                Ok(boxed(Container::new(fill(), vec![])?))
            })
            .unwrap(),
        )
    };
    let _outside = {
        let outside = outside.clone();
        effect(move || outside.borrow_mut().push(theme_core::use_control_size()))
    };

    canvas.set_control_size(Some(ControlSize::Mini));
    assert_eq!(canvas.control_size(), Some(ControlSize::Mini));
    theme_core::set_control_size(ControlSize::Large);
    canvas.set_control_size(None);
    theme_core::set_control_size(ControlSize::Small);

    assert_eq!(
        *inside.borrow(),
        vec![
            ControlSize::Regular,
            ControlSize::Mini,
            ControlSize::Large,
            ControlSize::Small
        ],
        "its own size, deaf to the window's change, then the window's again once cleared"
    );
    assert_eq!(
        *outside.borrow(),
        vec![ControlSize::Regular, ControlSize::Large, ControlSize::Small],
        "the window never sees the frame's size"
    );
    theme_core::set_control_size(ControlSize::Regular);
}

#[test]
fn a_modal_open_from_the_start_covers_its_frame_above_the_page() {
    reset_layout_runtime();
    let canvas = Rc::new(
        SurfaceCanvas::new(Size::new(1.0, 1.0), || {
            let scrim = swatch(Color::BLACK, fill())?;
            let modal = Overlay::toggleable(LayoutStyle::new(), vec![boxed(scrim)], || true)?;
            let declared_in = Container::new(
                LayoutStyle::new().width(40.0).height(20.0),
                vec![boxed(modal)],
            )?;
            let page = swatch(Color::WHITE, LayoutStyle::new().height(30.0))?;
            Ok(boxed(Container::new(
                fill().flex_column(),
                vec![boxed(page), boxed(declared_in)],
            )?))
        })
        .unwrap(),
    );
    let window = Window::new(
        SurfaceFrame::filling(
            Rc::clone(&canvas),
            LayoutStyle::new().width(FRAME.width).height(FRAME.height),
        )
        .unwrap(),
        no_focusable(),
        no_focusable(),
        || {},
    );

    let drawn = window.frame();
    assert_eq!(
        fills(&drawn, Color::BLACK),
        [(FRAME, Some(FRAME))],
        "the scrim spans the frame's surface, not the 40×20 box it was declared in"
    );
    let order = |color: Color| {
        drawn
            .iter()
            .position(|command| {
                matches!(command, DrawCommand::Rect { style, .. } if style.fill == Some(color.into()))
            })
            .unwrap()
    };
    assert!(
        order(Color::BLACK) > order(Color::WHITE),
        "the modal is drawn above the page"
    );
}
