//! Terminal input, in Telar's event vocabulary.

use crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use platform_core::{
    Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource, ScrollDelta,
};

/// Where a cell sits in the logical pixel space layout works in — its centre, so a click lands unambiguously inside the cell the user aimed at rather than on its boundary.
pub fn cell_to_logical(col: u16, row: u16, cell_width: f32, cell_height: f32) -> (f64, f64) {
    (
        ((col as f32 + 0.5) * cell_width) as f64,
        ((row as f32 + 0.5) * cell_height) as f64,
    )
}

/// Translates crossterm's modifier flags into Telar's.
pub fn map_modifiers(m: KeyModifiers) -> ModifiersState {
    ModifiersState {
        is_shift: m.contains(KeyModifiers::SHIFT),
        is_ctrl: m.contains(KeyModifiers::CONTROL),
        is_alt: m.contains(KeyModifiers::ALT),
        is_meta: m.contains(KeyModifiers::SUPER),
    }
}

/// Translates a crossterm key code, returning `None` for one Telar has no spelling for.
pub fn map_key(code: KeyCode) -> Option<Key> {
    let named = |k| Some(Key::Named(k));
    match code {
        KeyCode::Char(c) => Some(Key::Char(c)),
        KeyCode::Enter => named(NamedKey::Enter),
        KeyCode::Backspace => named(NamedKey::Backspace),
        KeyCode::Esc => named(NamedKey::Escape),
        KeyCode::Tab | KeyCode::BackTab => named(NamedKey::Tab),
        KeyCode::Delete => named(NamedKey::Delete),
        KeyCode::Home => named(NamedKey::Home),
        KeyCode::End => named(NamedKey::End),
        KeyCode::PageUp => named(NamedKey::PageUp),
        KeyCode::PageDown => named(NamedKey::PageDown),
        KeyCode::Up => named(NamedKey::ArrowUp),
        KeyCode::Down => named(NamedKey::ArrowDown),
        KeyCode::Left => named(NamedKey::ArrowLeft),
        KeyCode::Right => named(NamedKey::ArrowRight),
        KeyCode::Insert => named(NamedKey::Insert),
        KeyCode::CapsLock => named(NamedKey::CapsLock),
        KeyCode::F(n) => function_key(n),
        _ => None,
    }
}

fn function_key(n: u8) -> Option<Key> {
    const KEYS: [NamedKey; 24] = [
        NamedKey::F1,
        NamedKey::F2,
        NamedKey::F3,
        NamedKey::F4,
        NamedKey::F5,
        NamedKey::F6,
        NamedKey::F7,
        NamedKey::F8,
        NamedKey::F9,
        NamedKey::F10,
        NamedKey::F11,
        NamedKey::F12,
        NamedKey::F13,
        NamedKey::F14,
        NamedKey::F15,
        NamedKey::F16,
        NamedKey::F17,
        NamedKey::F18,
        NamedKey::F19,
        NamedKey::F20,
        NamedKey::F21,
        NamedKey::F22,
        NamedKey::F23,
        NamedKey::F24,
    ];
    KEYS.get(n.checked_sub(1)? as usize)
        .cloned()
        .map(Key::Named)
}

fn map_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Auxiliary,
    }
}

/// What one terminal event means, as zero or more Telar events.
///
/// More than one because a terminal folds things Telar keeps apart: a key event also carries the current modifiers, and a paste is a run of characters. Zero because a terminal reports events — a key nothing maps to, a mouse kind with no equivalent — that are not worth inventing a meaning for.
pub struct Mapper {
    modifiers: ModifiersState,
    cell_width: f32,
    cell_height: f32,
    /// Where the pointer was last seen, so a scroll can carry a position the way every other backend's does.
    pointer: (f64, f64),
    /// Whether the terminal reports key releases. When it does not, a press is synthesised into a matching release immediately, so nothing upstream is left holding a key that will never come up.
    synthesises_releases: bool,
}

impl Mapper {
    pub fn new(cell_width: f32, cell_height: f32, reports_key_releases: bool) -> Self {
        Self {
            modifiers: ModifiersState::default(),
            cell_width,
            cell_height,
            pointer: (0.0, 0.0),
            synthesises_releases: !reports_key_releases,
        }
    }

    pub fn map(&mut self, event: TermEvent, out: &mut Vec<Event>) {
        match event {
            TermEvent::Key(key) => self.key(key, out),
            TermEvent::Mouse(mouse) => self.mouse(mouse, out),
            TermEvent::Resize(cols, rows) => out.push(Event::WindowResized {
                width: (cols as f32 * self.cell_width).round() as u32,
                height: (rows as f32 * self.cell_height).round() as u32,
            }),
            TermEvent::FocusGained => out.push(Event::FocusChanged { is_focused: true }),
            TermEvent::FocusLost => out.push(Event::FocusChanged { is_focused: false }),
            // A paste arrives as one string; upstream only knows about typing, so it is typed.
            TermEvent::Paste(text) => {
                for c in text.chars() {
                    out.push(Event::KeyPressed {
                        key: Key::Char(c),
                        modifiers: ModifiersState::default(),
                    });
                    out.push(Event::KeyReleased {
                        key: Key::Char(c),
                        modifiers: ModifiersState::default(),
                    });
                }
            }
        }
    }

    fn key(&mut self, key: KeyEvent, out: &mut Vec<Event>) {
        let modifiers = map_modifiers(key.modifiers);
        if modifiers != self.modifiers {
            self.modifiers = modifiers;
            out.push(Event::ModifiersChanged { modifiers });
        }
        let Some(mapped) = map_key(key.code) else {
            return;
        };
        match key.kind {
            KeyEventKind::Press | KeyEventKind::Repeat => {
                out.push(Event::KeyPressed {
                    key: mapped.clone(),
                    modifiers,
                });
                if self.synthesises_releases {
                    out.push(Event::KeyReleased {
                        key: mapped,
                        modifiers,
                    });
                }
            }
            KeyEventKind::Release => out.push(Event::KeyReleased {
                key: mapped,
                modifiers,
            }),
        }
    }

    fn mouse(&mut self, mouse: MouseEvent, out: &mut Vec<Event>) {
        let (x, y) = cell_to_logical(mouse.column, mouse.row, self.cell_width, self.cell_height);
        self.pointer = (x, y);
        let modifiers = map_modifiers(mouse.modifiers);
        if modifiers != self.modifiers {
            self.modifiers = modifiers;
            out.push(Event::ModifiersChanged { modifiers });
        }
        match mouse.kind {
            MouseEventKind::Moved => out.push(Event::PointerMoved {
                x,
                y,
                source: PointerSource::Mouse,
            }),
            // A drag is a move with a button held; Telar tracks the button itself, so it is just a move.
            MouseEventKind::Drag(_) => out.push(Event::PointerMoved {
                x,
                y,
                source: PointerSource::Mouse,
            }),
            MouseEventKind::Down(button) => {
                // A terminal reports the press at its cell without a preceding move, so a widget that has never seen the pointer would take a click it never highlighted for.
                out.push(Event::PointerMoved {
                    x,
                    y,
                    source: PointerSource::Mouse,
                });
                out.push(Event::PointerPressed {
                    x,
                    y,
                    button: map_button(button),
                    source: PointerSource::Mouse,
                });
            }
            MouseEventKind::Up(button) => out.push(Event::PointerReleased {
                x,
                y,
                button: map_button(button),
                source: PointerSource::Mouse,
            }),
            MouseEventKind::ScrollUp => out.push(self.scroll(0.0, 1.0)),
            MouseEventKind::ScrollDown => out.push(self.scroll(0.0, -1.0)),
            MouseEventKind::ScrollLeft => out.push(self.scroll(1.0, 0.0)),
            MouseEventKind::ScrollRight => out.push(self.scroll(-1.0, 0.0)),
        }
    }

    fn scroll(&self, x: f32, y: f32) -> Event {
        Event::Scrolled {
            delta: ScrollDelta::Lines { x, y },
            x: self.pointer.0,
            y: self.pointer.1,
        }
    }
}

#[cfg(test)]
mod tests {
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
        assert!(matches!(out[0], Event::PointerMoved { x, y, .. } if x == 28.0 && y == 40.0));
        assert!(matches!(out[1], Event::PointerPressed { x, y, .. } if x == 28.0 && y == 40.0));
    }

    #[test]
    fn a_terminal_without_key_releases_gets_synthetic_ones() {
        let mut m = Mapper::new(8.0, 16.0, false);
        let out = events(
            &mut m,
            TermEvent::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        );
        assert!(matches!(out[0], Event::KeyPressed { .. }));
        assert!(matches!(out[1], Event::KeyReleased { .. }));
    }

    #[test]
    fn a_terminal_with_key_releases_gets_only_what_it_reported() {
        let out = events(
            &mut mapper(),
            TermEvent::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        );
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], Event::KeyPressed { .. }));
    }

    #[test]
    fn a_modifier_change_is_announced_once() {
        let mut m = mapper();
        let first = events(
            &mut m,
            TermEvent::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        );
        assert!(matches!(first[0], Event::ModifiersChanged { .. }));
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
        assert!(matches!(
            out[0],
            Event::KeyPressed {
                key: Key::Char('h'),
                ..
            }
        ));
        assert!(matches!(
            out[2],
            Event::KeyPressed {
                key: Key::Char('i'),
                ..
            }
        ));
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
        assert!(matches!(out[0], Event::Scrolled { x, .. } if x == 44.0));
    }

    #[test]
    fn every_function_key_maps() {
        for n in 1..=24u8 {
            assert!(map_key(KeyCode::F(n)).is_some(), "F{n} did not map");
        }
        assert!(map_key(KeyCode::F(25)).is_none());
    }
}
