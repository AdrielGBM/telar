//! An embedded guest the `guest_contract` test in `telar` loads into a host with its own copy of std and an allocator marked apart from this one's: it draws text and an image, takes moves and keys, has a text field and a button a screen reader, the keyboard and the input method reach through its host, queues window commands, waits on a timer, wakes its host, and panics on a press, so every entry of the embed contract runs.

use std::sync::Arc;
use std::time::Duration;

use telar::{
    AppCtx, Color, Container, DrawCommand, Event, EventResult, ImageData, Input, Key, LayoutItem,
    LayoutStyle, NodeId, PointerButton, Rect, RectStyle, RenderNode, RwSignal, StyledContainer,
    Text, TextStyle, Timer, WindowCommand, box_item, focus, push_window_command, run_after,
    signal,
};
use telar_embed::EmbeddedApp;

#[path = "../../tagged_alloc.rs"]
mod tagged_alloc;

/// An allocator of its own, marked apart from the host's, so a block that crosses the contract owned and is freed on the other side stops the process.
#[global_allocator]
static ALLOCATOR: tagged_alloc::Tagged = tagged_alloc::Tagged(0x656d_6265_6467_7374);

/// How long after it is built the guest's timer comes due.
const TIMER_DUE_IN: Duration = Duration::from_secs(60);

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
            LayoutStyle::new().flex_column(),
            vec![box_item(label), box_item(field), box_item(button)],
        )
        .expect("the form builds");
        self.content = Some(box_item(form));
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
            Event::PointerPressed {
                button: PointerButton::Primary,
                ..
            } => panic!("RUNTIME is already borrowed by the press handler"),
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
