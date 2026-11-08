//! That an overlay of your own can be installed through the facade alone.
//!
//! `DevOverlay` was public and the frame loop generic over it for as long as both existed, and neither fact was reachable: every entry point that opens a window named the built-in overlay, and installing another meant depending on `telar-platform-desktop` and building the platform by hand. This is the door, and it is a compile test because what it asserts is that the *types* line up — running it would open a window.

use std::borrow::Cow;

use telar::{
    AccessNode, DevAction, DevOverlay, DrawCommand, Event, OverlayResponse, SegmentNodeInfo,
};

/// The smallest overlay that is not `()`: it counts the frames it was handed, claims the presses over its strip at the top, and reads the accessibility tree while it is open.
#[derive(Default)]
struct Ruler {
    frames: usize,
    open: bool,
    controls: usize,
}

impl DevOverlay for Ruler {
    fn on_frame<'a>(
        &mut self,
        base: &'a [DrawCommand],
        _window_w: f32,
        _window_h: f32,
        _tree_dirty: bool,
    ) -> Cow<'a, [DrawCommand]> {
        self.frames += 1;
        Cow::Borrowed(base)
    }

    fn on_event(&mut self, event: &Event) -> OverlayResponse {
        match event {
            Event::PointerPressed { y, .. } if *y < 20.0 => {
                self.open = !self.open;
                OverlayResponse {
                    consumed: true,
                    action: Some(DevAction::Redraw),
                }
            }
            _ => OverlayResponse::IGNORED,
        }
    }

    fn needs_frame(&self) -> bool {
        self.open
    }

    fn wants_access(&self) -> bool {
        self.open
    }

    fn on_access(&mut self, nodes: &[AccessNode]) {
        self.controls = nodes.iter().filter(|n| n.id.is_some()).count();
    }

    fn on_tree(&mut self, _nodes: &[SegmentNodeInfo]) {}
}

struct Blank;

impl telar::App for Blank {
    fn root(&self) -> Box<dyn telar::Component> {
        unreachable!("this test never runs the app, it only type-checks the door")
    }
}

#[test]
fn an_overlay_of_your_own_installs_through_the_facade_alone() {
    #[allow(dead_code)]
    fn _install() {
        telar::run_app_with_devtools::<Blank, Ruler>(
            telar::AppConfig::default(),
            Blank,
            "door-test",
        );
    }
}

#[test]
fn an_overlay_of_your_own_answers_events_through_the_facade_types() {
    let mut ruler = Ruler::default();
    let strip = Event::PointerPressed {
        x: 4.0,
        y: 4.0,
        button: telar::PointerButton::Primary,
        source: telar::PointerSource::Mouse,
    };
    assert!(ruler.on_event(&strip).consumed);
    assert!(ruler.needs_frame() && ruler.wants_access());
    let moved = Event::PointerMoved {
        x: 4.0,
        y: 4.0,
        source: telar::PointerSource::Mouse,
    };
    assert_eq!(ruler.on_event(&moved), OverlayResponse::IGNORED);
    ruler.on_access(&[]);
    assert_eq!(ruler.controls, 0);
}
