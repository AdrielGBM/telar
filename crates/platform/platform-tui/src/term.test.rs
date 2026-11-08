use std::sync::{Arc, Mutex};

use super::*;

#[derive(Clone, Default)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct RefusingWriter;

impl Write for RefusingWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("closed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("closed"))
    }
}

#[derive(Clone, Default)]
struct FakeRawMode {
    raw: Arc<AtomicBool>,
}

impl FakeRawMode {
    fn is_raw(&self) -> bool {
        self.raw.load(Ordering::SeqCst)
    }
}

impl RawMode for FakeRawMode {
    fn enable(&mut self) -> io::Result<()> {
        self.raw.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn disable(&mut self) -> io::Result<()> {
        self.raw.store(false, Ordering::SeqCst);
        Ok(())
    }
}

const UNDO_MOUSE: &str = "\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l";
const UNDO_REST: &str = "\x1b[?1004l\x1b[?2004l\x1b[?25h\x1b[?1049l\x1b[23;0t";

#[test]
fn leaving_writes_the_exact_restore_sequences_and_ends_raw_mode() {
    let active = AtomicBool::new(false);
    let raw = FakeRawMode::default();
    let out = SharedBuffer::default();
    let session = Session::enter_with(&active, raw.clone(), out.clone(), true, || false).unwrap();
    assert!(raw.is_raw());
    out.take();

    drop(session);

    assert_eq!(
        String::from_utf8(out.take()).unwrap(),
        format!("{UNDO_MOUSE}{UNDO_REST}")
    );
    assert!(!raw.is_raw());
    assert!(!active.load(Ordering::SeqCst));
}

#[test]
fn leaving_pops_keyboard_enhancement_first_when_it_was_pushed() {
    let active = AtomicBool::new(false);
    let out = SharedBuffer::default();
    let session =
        Session::enter_with(&active, FakeRawMode::default(), out.clone(), false, || true).unwrap();
    assert!(session.reports_key_releases());
    out.take();

    drop(session);

    assert_eq!(
        String::from_utf8(out.take()).unwrap(),
        format!("\x1b[<1u\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l{UNDO_REST}")
    );
}

#[test]
fn entering_writes_the_application_mode_sequences() {
    let active = AtomicBool::new(false);
    let out = SharedBuffer::default();
    let session = Session::enter_with(&active, FakeRawMode::default(), out.clone(), false, || {
        false
    })
    .unwrap();
    assert_eq!(
        String::from_utf8(out.take()).unwrap(),
        "\x1b[?1049h\x1b[?25l\x1b[?2004h\x1b[?1004h\x1b[22;0t"
    );
    drop(session);
}

#[test]
fn a_failure_after_raw_mode_still_restores_the_terminal() {
    let active = AtomicBool::new(false);
    let raw = FakeRawMode::default();

    let entered = Session::enter_with(&active, raw.clone(), RefusingWriter, true, || false);

    assert!(entered.is_err());
    assert!(!raw.is_raw());
    assert!(!active.load(Ordering::SeqCst));
}

#[test]
fn leaving_twice_writes_nothing_the_second_time() {
    let active = AtomicBool::new(true);
    let raw = FakeRawMode::default();
    let out = SharedBuffer::default();
    let mut writer = out.clone();
    let mut switch = raw.clone();

    leave(&active, &mut switch, &mut writer, false);
    out.take();
    leave(&active, &mut switch, &mut writer, false);

    assert!(out.take().is_empty());
}
