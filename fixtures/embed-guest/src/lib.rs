//! An embedded guest the `guest_contract` test in `telar` loads into a host with its own copy of std and an allocator marked apart from this one's: it draws text and an image, takes moves and keys, has a text field and a button a screen reader, the keyboard and the input method reach through its host, queues window commands, lists the font families it sets text in, waits on a timer, wakes its host, starts drags from a pad that takes them too, and panics on a press nothing in it takes, so every entry of the embed contract runs.

use std::sync::Arc;
use std::time::Duration;

use telar::{
    AppCtx, Color, Container, DragActions, DragData, DrawCommand, DropTarget, Event, EventResult,
    ImageData, Input, Key, LayoutItem, LayoutStyle, MimeType, NodeId, OutgoingDrag, PointerButton,
    Rect, RectStyle, RenderNode, RwSignal, StyledContainer, Text, TextStyle, Timer, WindowCommand,
    box_item, focus, push_window_command, run_after, signal,
};
use telar_embed::EmbeddedApp;

#[path = "../../tagged_alloc.rs"]
mod tagged_alloc;

/// An allocator of its own, marked apart from the host's, so a block that crosses the contract owned and is freed on the other side stops the process.
#[global_allocator]
static ALLOCATOR: tagged_alloc::Tagged = tagged_alloc::Tagged(0x656d_6265_6467_7374);

/// How long after it is built the guest's timer comes due.
const TIMER_DUE_IN: Duration = Duration::from_secs(60);

/// How wide the column of the label, the field and the button is; the drag pad sits right of it, from the top.
const FORM_WIDTH: f32 = 140.0;
const PAD_SIZE: f32 = 60.0;

/// The type the pad's drags carry and the pad takes drops of.
const TOKEN: MimeType = MimeType::from_static("application/x-telar-embed-guest");

/// A box a drag carrying the guest's title starts from, and that takes such a drag, saying through a window command what it took and how the drag it started ended.
fn drag_pad(title: &str) -> StyledContainer {
    let (dragged, ended, took) = (title.to_owned(), title.to_owned(), title.to_owned());
    StyledContainer::new(
        LayoutStyle::new().width(PAD_SIZE).height(PAD_SIZE),
        |_| RectStyle::default(),
        vec![],
    )
    .expect("the pad builds")
    .drag_source(
        move || {
            Some(OutgoingDrag::new(
                DragData::new().with(TOKEN, dragged.as_bytes()),
                DragActions::Copy,
            ))
        },
        move |outcome| {
            push_window_command(WindowCommand::SetTitle(format!(
                "{ended} ended {outcome:?}"
            )))
        },
    )
    .drop_target(
        DropTarget::new([TOKEN]),
        |_| {},
        move |data, _, _| {
            push_window_command(WindowCommand::SetTitle(format!(
                "{took} took {}",
                String::from_utf8_lossy(&data.bytes)
            )))
        },
    )
}

struct Fixture {
    title: String,
    moves: RwSignal<u32>,
    content: Option<Box<dyn LayoutItem>>,
    picture: Arc<ImageData>,
    _timer: Option<Timer>,
}

impl Fixture {
    fn new(args: &[String]) -> Self {
        Self {
            title: args.join(" "),
            moves: signal(0),
            content: None,
            picture: Arc::new(ImageData::new(vec![0x80; 4 * 4 * 4], 4, 4)),
            _timer: None,
        }
    }
}

impl EmbeddedApp for Fixture {
    fn build(&mut self) {
        telar::install_default_text_metrics();
        let moves = self.moves;
        let label = Text::new(
            move || format!("moved over {} times", moves.get()),
            LayoutStyle::new(),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .expect("the label builds");
        let field = Input::new(
            signal(String::from("ab")),
            LayoutStyle::new().width(120.0).height(20.0),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .expect("the field builds");
        let presses = signal(0);
        let button = StyledContainer::new(
            LayoutStyle::new().width(60.0).height(24.0),
            |_| RectStyle::default(),
            vec![],
        )
        .expect("the button builds")
        .control(focus::Role::Button)
        .on_press(move || {
            presses.update(|presses| *presses += 1);
            push_window_command(WindowCommand::SetTitle(format!(
                "pressed {}",
                presses.peek()
            )));
        });
        let form = Container::new(
            LayoutStyle::new().flex_column().width(FORM_WIDTH),
            vec![box_item(label), box_item(field), box_item(button)],
        )
        .expect("the form builds");
        let row = Container::new(
            LayoutStyle::new().flex_row(),
            vec![box_item(form), box_item(drag_pad(&self.title))],
        )
        .expect("the row builds");
        self.content = Some(box_item(row));
        self._timer = Some(run_after(TIMER_DUE_IN, || {}));
    }

    fn layout_root(&self) -> NodeId {
        self.content.as_ref().expect("built").layout_node()
    }

    fn view(&self) -> RenderNode {
        let picture = RenderNode::Primitive(DrawCommand::Image {
            data: Arc::clone(&self.picture),
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
            raster: Default::default(),
            fill: Default::default(),
        });
        RenderNode::group([self.content.as_ref().expect("built").view(), picture])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::PointerMoved { .. } => {
                self.moves.update(|moves| *moves += 1);
                self.content.as_mut().expect("built").on_event(event);
                EventResult::Handled
            }
            Event::KeyPressed {
                key: Key::Char('t'),
                ..
            } => {
                push_window_command(WindowCommand::SetTitle(format!(
                    "{} moved {} times",
                    self.title,
                    self.moves.peek()
                )));
                EventResult::Handled
            }
            Event::KeyPressed {
                key: Key::Char('f'),
                ..
            } => {
                push_window_command(WindowCommand::SetTitle(telar::font_families().join("\n")));
                EventResult::Handled
            }
            Event::PointerPressed {
                button: PointerButton::Primary,
                ..
            } => match self.content.as_mut().expect("built").on_event(event) {
                EventResult::Handled => EventResult::Handled,
                EventResult::Ignored => panic!("RUNTIME is already borrowed by the press handler"),
            },
            _ => self.content.as_mut().expect("built").on_event(event),
        }
    }

    fn on_frame(&mut self, ctx: &mut AppCtx) {
        if let Some(waker) = ctx.redraw_waker() {
            waker.wake();
        }
        ctx.request_redraw();
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Color::rgb(0.1, 0.2, 0.3))
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn icon(&self) -> Option<Vec<u8>> {
        Some(vec![1, 2, 3, 4])
    }

    fn id(&self) -> String {
        "embed-guest".to_owned()
    }
}

telar_embed::embed!(|args: &[String]| -> Box<dyn EmbeddedApp> { Box::new(Fixture::new(args)) });
