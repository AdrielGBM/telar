use super::*;

fn mapper() -> Mapper {
    Mapper::new(8.0, 16.0, true)
}

fn events(mapper: &mut Mapper, event: TermEvent) -> Vec<Event> {
    let mut out = Vec::new();
    mapper.map(event, &mut out);
    out
}

#[test]
fn a_resize_is_reported_in_logical_pixels() {
    let out = events(&mut mapper(), TermEvent::Resize(80, 24));
    assert_eq!(
        out,
        vec![Event::WindowResized {
            width: 640,
            height: 384
        }]
    );
}

#[test]
fn a_click_lands_at_the_centre_of_its_cell() {
    let out = events(
        &mut mapper(),
        TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 2,
            modifiers: KeyModifiers::NONE,
        }),
    );
    assert!(
        matches!(out[0], Event::PointerMoved { x, y, .. } if x == 28.0 && y == 40.0),
        "a press first moves the pointer to the cell centre: {:?}",
        out[0]
    );
    assert!(
        matches!(out[1], Event::PointerPressed { x, y, .. } if x == 28.0 && y == 40.0),
        "and presses there: {:?}",
        out[1]
    );
}

#[test]
fn a_terminal_without_key_releases_gets_synthetic_ones() {
    let mut m = Mapper::new(8.0, 16.0, false);
    let out = events(
        &mut m,
        TermEvent::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    assert!(
        matches!(out[0], Event::KeyPressed { .. }),
        "the reported press comes through: {:?}",
        out[0]
    );
    assert!(
        matches!(out[1], Event::KeyReleased { .. }),
        "and a release is synthesised for it: {:?}",
        out[1]
    );
}

#[test]
fn a_terminal_with_key_releases_gets_only_what_it_reported() {
    let out = events(
        &mut mapper(),
        TermEvent::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
    );
    assert_eq!(out.len(), 1);
    assert!(
        matches!(out[0], Event::KeyPressed { .. }),
        "a terminal that reports releases gets no synthetic one: {out:?}"
    );
}

#[test]
fn a_modifier_change_is_announced_once() {
    let mut m = mapper();
    let first = events(
        &mut m,
        TermEvent::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
    );
    assert!(
        matches!(first[0], Event::ModifiersChanged { .. }),
        "the first modifier change is announced: {:?}",
        first[0]
    );
    let second = events(
        &mut m,
        TermEvent::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL)),
    );
    assert!(
        !second
            .iter()
            .any(|e| matches!(e, Event::ModifiersChanged { .. })),
        "unchanged modifiers must not be re-announced"
    );
}

#[test]
fn a_paste_is_typed() {
    let out = events(&mut mapper(), TermEvent::Paste("hi".into()));
    assert_eq!(out.len(), 4);
    assert!(
        matches!(
            out[0],
            Event::KeyPressed {
                key: Key::Char('h'),
                ..
            }
        ),
        "a paste arrives as typed text: {out:?}"
    );
    assert!(
        matches!(
            out[2],
            Event::KeyPressed {
                key: Key::Char('i'),
                ..
            }
        ),
        "and the rest of it too: {out:?}"
    );
}

#[test]
fn a_scroll_carries_the_last_pointer_position() {
    let mut m = mapper();
    events(
        &mut m,
        TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 5,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }),
    );
    let out = events(
        &mut m,
        TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }),
    );
    assert!(
        matches!(out[0], Event::Scrolled { x, .. } if x == 44.0),
        "a scroll reuses the last pointer position: {:?}",
        out[0]
    );
}

#[test]
fn every_function_key_maps() {
    for n in 1..=24u8 {
        assert!(map_key(KeyCode::F(n)).is_some(), "F{n} did not map");
    }
    assert!(map_key(KeyCode::F(25)).is_none(), "there is no F25 to map");
}
