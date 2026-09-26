use std::cell::RefCell;

use geometry_core::Rect;
use platform_core::{AccessNode, Role};

use super::*;

struct Screen(RefCell<Vec<AccessNode>>);

impl EventHandler<TuiWindow> for Screen {
    fn on_resume(&mut self, _window: &TuiWindow) -> bool {
        true
    }
    fn on_event(&mut self, _event: Event, _window: &TuiWindow) {}
    fn on_redraw(&mut self, _window: &TuiWindow) {}
    fn accessibility(&self) -> Vec<AccessNode> {
        self.0.borrow().clone()
    }
}

fn label(name: &str) -> AccessNode {
    AccessNode {
        id: None,
        role: Role::Label,
        name: name.to_string(),
        rect: Rect::default(),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        lang: None,
        url: None,
    }
}

/// The reading follows the screen: what an application named is what the file says, and a change on screen is a change in the file.
#[test]
fn the_reading_is_rewritten_when_the_screen_changes() {
    let path = std::env::temp_dir().join(format!("telar-reading-{}.txt", std::process::id()));
    let screen = Screen(RefCell::new(vec![label("Hola")]));
    let mut reading = Reading::new(Some(path.clone()));

    reading.follow(&screen);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hola");

    screen.0.borrow_mut().push(label("mundo"));
    reading.follow(&screen);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "Hola\nmundo");
    let _ = std::fs::remove_file(&path);
}

/// Off unless asked for: a frame costs no snapshot when nobody reads it.
#[test]
fn no_path_reads_nothing() {
    struct Untouchable;
    impl EventHandler<TuiWindow> for Untouchable {
        fn on_resume(&mut self, _window: &TuiWindow) -> bool {
            true
        }
        fn on_event(&mut self, _event: Event, _window: &TuiWindow) {}
        fn on_redraw(&mut self, _window: &TuiWindow) {}
        fn accessibility(&self) -> Vec<AccessNode> {
            panic!("the snapshot is only built for a reading someone asked for")
        }
    }
    Reading::new(None).follow(&Untouchable);
}
