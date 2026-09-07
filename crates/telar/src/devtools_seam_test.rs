//! That an overlay of your own can be installed through the facade alone.
//!
//! `DevPlugin` was public and the frame loop generic over it for as long as both existed, and neither fact was reachable: every entry point that opens a window named the built-in overlay, and installing another meant depending on `telar-platform-desktop` and building the platform by hand. This is the door, and it is a compile test because what it asserts is that the *types* line up — running it would open a window.

use std::borrow::Cow;
use std::time::Duration;

use telar::{DevAction, DevPlugin, DrawCommand, Key, ModifiersState, SegmentNodeInfo};

/// The smallest overlay that is not `()`: it counts the frames it was handed and draws nothing.
#[derive(Default)]
struct Ruler {
    frames: usize,
}

impl DevPlugin for Ruler {
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

    fn keepalive_interval(&self) -> Option<Duration> {
        None
    }

    fn on_key(&mut self, _key: &Key, _modifiers: ModifiersState) -> DevAction {
        DevAction::None
    }

    fn on_pointer_pressed(&mut self, _x: f32, _y: f32) -> bool {
        false
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
