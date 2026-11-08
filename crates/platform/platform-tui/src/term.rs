//! Putting the terminal into the state an application draws on, and — whatever happens — putting it back.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    supports_keyboard_enhancement,
};
use crossterm::{cursor, execute};

// XTWINOPS: push the icon name and window title on entry and pop them on the way out, so the titles the app sets leave with it. A terminal without a title stack ignores both.
const SAVE_TITLE: &[u8] = b"\x1b[22;0t";
const RESTORE_TITLE: &[u8] = b"\x1b[23;0t";

/// Whether a terminal is currently in application mode. Read by the panic hook, which has no other way to know whether there is anything to undo — and undoing nothing must be harmless, because the hook runs on every panic in the process.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// The terminal's raw-mode switch, apart from the escape sequences, so entering and leaving run against a buffer and a fake switch exactly as they run against the real terminal.
pub trait RawMode {
    fn enable(&mut self) -> io::Result<()>;
    fn disable(&mut self) -> io::Result<()>;
}

pub struct CrosstermRawMode;

impl RawMode for CrosstermRawMode {
    fn enable(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }

    fn disable(&mut self) -> io::Result<()> {
        disable_raw_mode()
    }
}

/// Owns the terminal's application mode. Restoring is done by `Drop` **and** by a panic hook, because a panic that unwinds past the drop — or one in a thread that aborts the process — would otherwise leave the user with a terminal in raw mode, no cursor, and no echo: a shell they cannot type into.
pub struct Session<'a, R: RawMode, W: Write> {
    active: &'a AtomicBool,
    raw: R,
    out: W,
    keyboard_enhanced: bool,
}

pub type TerminalMode = Session<'static, CrosstermRawMode, io::Stdout>;

impl TerminalMode {
    pub fn enter(mouse: bool) -> io::Result<Self> {
        install_panic_hook();
        Session::enter_with(&ACTIVE, CrosstermRawMode, io::stdout(), mouse, || {
            supports_keyboard_enhancement().unwrap_or(false)
        })
    }
}

impl<'a, R: RawMode, W: Write> Session<'a, R, W> {
    pub fn enter_with(
        active: &'a AtomicBool,
        mut raw: R,
        out: W,
        mouse: bool,
        supports_keyboard_enhancement: impl FnOnce() -> bool,
    ) -> io::Result<Self> {
        raw.enable()?;
        active.store(true, Ordering::SeqCst);
        let mut session = Self {
            active,
            raw,
            out,
            keyboard_enhanced: false,
        };
        session.begin(mouse, supports_keyboard_enhancement)?;
        Ok(session)
    }

    fn begin(
        &mut self,
        mouse: bool,
        supports_keyboard_enhancement: impl FnOnce() -> bool,
    ) -> io::Result<()> {
        let out = &mut self.out;
        execute!(
            out,
            EnterAlternateScreen,
            cursor::Hide,
            EnableBracketedPaste,
            EnableFocusChange
        )?;
        out.write_all(SAVE_TITLE)?;
        if mouse {
            execute!(out, EnableMouseCapture)?;
        }
        // Key *release* and unambiguous modifier reporting only exist under the kitty keyboard protocol. Without it a terminal reports presses alone, which is why the event mapping cannot assume releases.
        if supports_keyboard_enhancement() {
            execute!(
                out,
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                )
            )?;
            self.keyboard_enhanced = true;
        }
        out.flush()
    }

    /// Whether the terminal reports key releases. A backend that assumes it does on a terminal that does not leaves every key stuck down.
    pub fn reports_key_releases(&self) -> bool {
        self.keyboard_enhanced
    }
}

impl<R: RawMode, W: Write> Drop for Session<'_, R, W> {
    fn drop(&mut self) {
        leave(
            self.active,
            &mut self.raw,
            &mut self.out,
            self.keyboard_enhanced,
        );
    }
}

/// Undoes everything `TerminalMode::enter` did. Idempotent, and safe to call having entered nothing.
pub fn restore() {
    leave(&ACTIVE, &mut CrosstermRawMode, &mut io::stdout(), false);
}

fn leave<R: RawMode, W: Write>(
    active: &AtomicBool,
    raw: &mut R,
    out: &mut W,
    keyboard_enhanced: bool,
) {
    if !active.swap(false, Ordering::SeqCst) {
        return;
    }
    if keyboard_enhanced {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    let _ = execute!(
        out,
        DisableMouseCapture,
        DisableFocusChange,
        DisableBracketedPaste,
        cursor::Show,
        LeaveAlternateScreen
    );
    let _ = out.write_all(RESTORE_TITLE);
    let _ = raw.disable();
    let _ = out.flush();
}

fn install_panic_hook() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
    });
}

/// The terminal's size in cells. Falls back to a conventional 80×24 when it cannot be asked — a pipe, or a terminal that answers nothing — so an app still lays out rather than collapsing to zero.
pub fn size() -> (u16, u16) {
    crossterm::terminal::size()
        .map(|(cols, rows)| (cols.max(1), rows.max(1)))
        .unwrap_or((80, 24))
}

#[cfg(test)]
#[path = "term.test.rs"]
mod tests;
