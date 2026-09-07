use super::*;
use geometry_core::Rect;

// Nothing here hands out an OS handle, which is the point: a window that has none is still a window.
struct TestWindow;
impl Window for TestWindow {
    fn width(&self) -> u32 {
        800
    }
    fn height(&self) -> u32 {
        600
    }
    fn request_redraw(&self) {}
}

struct TestHandler;
impl EventHandler<TestWindow> for TestHandler {
    fn on_resume(&mut self, _window: &TestWindow) -> bool {
        true
    }
    fn on_event(&mut self, _event: Event, _window: &TestWindow) {}
    fn on_redraw(&mut self, _window: &TestWindow) {}
    fn accessibility(&self) -> Vec<crate::AccessNode> {
        vec![crate::AccessNode {
            id: Some(1),
            role: crate::Role::Button,
            name: "test button".to_string(),
            rect: Rect::default(),
            focused: false,
            enabled: true,
            toggled: None,
            value: None,
        }]
    }
}

#[test]
fn boxed_handler_forwards_accessibility() {
    let handler = TestHandler;
    let boxed: Box<dyn EventHandler<TestWindow>> = Box::new(handler);
    let tree = boxed.accessibility();
    assert_eq!(
        tree.len(),
        1,
        "boxed handler should forward accessibility() and return non-empty tree"
    );
}
