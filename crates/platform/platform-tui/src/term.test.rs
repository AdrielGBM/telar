use std::sync::{Arc, Mutex};

use super::*;

#[derive(Clone, Copy, Default)]
enum Refusal {
    #[default]
    Nothing,
    Everything,
    Sequence(&'static str),
}

#[derive(Clone, Default)]
struct RecordingTerminal {
    written: Arc<Mutex<String>>,
    raw: Arc<AtomicBool>,
    keyboard_enhancement: bool,
    refusal: Refusal,
}

impl RecordingTerminal {
    fn with_keyboard_enhancement() -> Self {
        Self {
            keyboard_enhancement: true,
            ..Self::default()
        }
    }

    fn refusing(refusal: Refusal) -> Self {
        Self {
            refusal,
            ..Self::default()
        }
    }

    fn take(&self) -> String {
        std::mem::take(&mut self.written.lock().unwrap())
    }

    fn is_raw(&self) -> bool {
        self.raw.load(Ordering::SeqCst)
    }

    fn refuses(&self, sequence: &str) -> bool {
        match self.refusal {
            Refusal::Nothing => false,
            Refusal::Everything => true,
            Refusal::Sequence(refused) => sequence == refused,
        }
    }
}

impl Terminal for RecordingTerminal {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        self.raw.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        self.raw.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn supports_keyboard_enhancement(&mut self) -> bool {
        self.keyboard_enhancement
    }

    fn queue(&mut self, command: impl Command) -> io::Result<()> {
        let mut sequence = String::new();
        command
            .write_ansi(&mut sequence)
            .map_err(io::Error::other)?;
        if self.refuses(&sequence) {
            return Err(io::Error::other("refused"));
        }
        self.written.lock().unwrap().push_str(&sequence);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.refusal {
            Refusal::Everything => Err(io::Error::other("refused")),
            Refusal::Nothing | Refusal::Sequence(_) => Ok(()),
        }
    }
}

const UNDO_MOUSE: &str = "\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l";
const UNDO_REST: &str = "\x1b[?1004l\x1b[?2004l\x1b[?25h\x1b[?1049l\x1b[23;0t";

#[test]
fn leaving_writes_the_exact_restore_sequences_and_ends_raw_mode() {
    let entered = Entered::new();
    let terminal = RecordingTerminal::default();
    let session = Session::enter_with(&entered, terminal.clone(), true).unwrap();
    assert!(terminal.is_raw());
    terminal.take();

    drop(session);

    assert_eq!(terminal.take(), format!("{UNDO_MOUSE}{UNDO_REST}"));
    assert!(!terminal.is_raw());
}

#[test]
fn leaving_pops_keyboard_enhancement_first_when_it_was_pushed() {
    let entered = Entered::new();
    let terminal = RecordingTerminal::with_keyboard_enhancement();
    let session = Session::enter_with(&entered, terminal.clone(), true).unwrap();
    assert!(session.reports_key_releases());
    terminal.take();

    drop(session);

    assert_eq!(terminal.take(), format!("\x1b[<1u{UNDO_MOUSE}{UNDO_REST}"));
}

#[test]
fn a_terminal_without_keyboard_enhancement_is_never_asked_for_it() {
    let entered = Entered::new();
    let terminal = RecordingTerminal::default();

    let session = Session::enter_with(&entered, terminal.clone(), false).unwrap();
    drop(session);

    assert!(!terminal.take().contains("\x1b[>"));
}

#[test]
fn leaving_releases_only_the_mouse_it_captured() {
    let entered = Entered::new();
    let terminal = RecordingTerminal::default();
    let session = Session::enter_with(&entered, terminal.clone(), false).unwrap();
    terminal.take();

    drop(session);

    assert_eq!(terminal.take(), UNDO_REST);
}

#[test]
fn a_restore_step_that_fails_does_not_skip_the_ones_after_it() {
    let entered = Entered::new();
    let terminal = RecordingTerminal::refusing(Refusal::Sequence(UNDO_MOUSE));
    let session = Session::enter_with(&entered, terminal.clone(), true).unwrap();
    terminal.take();

    drop(session);

    assert_eq!(terminal.take(), UNDO_REST);
    assert!(!terminal.is_raw());
}

#[test]
fn entering_writes_the_application_mode_sequences() {
    let entered = Entered::new();
    let terminal = RecordingTerminal::default();
    let session = Session::enter_with(&entered, terminal.clone(), false).unwrap();
    assert_eq!(
        terminal.take(),
        "\x1b[?1049h\x1b[?25l\x1b[?2004h\x1b[?1004h\x1b[22;0t"
    );
    drop(session);
}

#[test]
fn a_failure_after_raw_mode_still_restores_the_terminal() {
    let entered = Entered::new();
    let terminal = RecordingTerminal::refusing(Refusal::Everything);

    let session = Session::enter_with(&entered, terminal.clone(), true);

    assert!(session.is_err());
    assert!(!terminal.is_raw());
}

#[test]
fn the_panic_hooks_restore_undoes_what_the_session_entered_and_leaves_its_drop_nothing() {
    let entered = Entered::new();
    let terminal = RecordingTerminal::with_keyboard_enhancement();
    let session = Session::enter_with(&entered, terminal.clone(), true).unwrap();
    terminal.take();

    leave(&entered, &mut terminal.clone());
    let restored = terminal.take();
    drop(session);

    assert_eq!(restored, format!("\x1b[<1u{UNDO_MOUSE}{UNDO_REST}"));
    assert!(terminal.take().is_empty());
    assert!(!terminal.is_raw());
}

#[test]
fn leaving_having_entered_nothing_writes_nothing() {
    let terminal = RecordingTerminal::default();

    leave(&Entered::new(), &mut terminal.clone());

    assert!(terminal.take().is_empty());
}
