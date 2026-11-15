use std::num::NonZeroU64;

use platform_core::{ElementId, Event, SurfaceKey};

use super::*;

const NESTED: SurfaceKey = SurfaceKey(NonZeroU64::new(3).unwrap());

#[test]
fn a_window_box_is_written_as_its_bare_id() {
    let key = BoxKey::from(42);
    assert_eq!(key.to_string(), "42");
    assert_eq!(BoxKey::parse("42"), Some(key));
}

#[test]
fn a_box_of_a_nested_surface_is_written_with_its_surface() {
    let key = BoxKey::new(Some(NESTED), 42);
    assert_eq!(key.to_string(), "3-42");
    assert_eq!(BoxKey::parse("3-42"), Some(key));
}

#[test]
fn what_no_key_wrote_is_no_key() {
    for written in ["", "-", "3-", "-42", "x", "3-x", "3-4-5"] {
        assert_eq!(BoxKey::parse(written), None, "{written:?}");
    }
}

#[test]
fn a_box_event_reaches_the_window_as_it_is_or_inside_the_surface_that_numbered_the_box() {
    let focused = |key: BoxKey| {
        key.event(Event::BoxFocused {
            box_id: key.element(),
        })
    };
    assert_eq!(
        focused(BoxKey::from(42)),
        Event::BoxFocused {
            box_id: ElementId(42)
        }
    );
    assert_eq!(
        focused(BoxKey::new(Some(NESTED), 42)),
        Event::InSurface {
            surface: NESTED,
            event: Box::new(Event::BoxFocused {
                box_id: ElementId(42),
            }),
        }
    );
}
