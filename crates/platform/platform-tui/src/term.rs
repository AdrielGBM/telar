//! Putting the terminal into the state an application draws on, and — whatever happens — putting it back.

use std::io::Write;
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

/// Whether a terminal is currently in application mode. Read by the panic hook, which has no other way to know whether there is anything to undo — and undoing nothing must be harmless, because the hook runs on every panic in the process.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Owns the terminal's application mode. Restoring is done by `Drop` **and** by a panic hook, because a panic that unwinds past the drop — or one in a thread that aborts the process — would otherwise leave the user with a terminal in raw mode, no cursor, and no echo: a shell they cannot type into.
pub struct TerminalMode {
    keyboard_enhanced: bool,
}

impl TerminalMode {
    pub fn enter(mouse: bool) -> std::io::Result<Self> {
        install_panic_hook();
        enable_raw_mode()?;
        let mut out = std::io::stdout();
        execute!(
            out,
            EnterAlternateScreen,
            cursor::Hide,
            EnableBracketedPaste,
            EnableFocusChange
        )?;
        if mouse {
            execute!(out, EnableMouseCapture)?;
        }
        // Key *release* and unambiguous modifier reporting only exist under the kitty keyboard protocol. Without it a terminal reports presses alone, which is why the event mapping cannot assume releases.
        let keyboard_enhanced = supports_keyboard_enhancement().unwrap_or(false);
        if keyboard_enhanced {
            execute!(
                out,
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                )
            )?;
        }
        out.flush()?;
        ACTIVE.store(true, Ordering::SeqCst);
        Ok(Self { keyboard_enhanced })
    }

    /// Whether the terminal reports key releases. A backend that assumes it does on a terminal that does not leaves every key stuck down.
    pub fn reports_key_releases(&self) -> bool {
        self.keyboard_enhanced
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        if self.keyboard_enhanced {
            let _ = execute!(std::io::stdout(), PopKeyboardEnhancementFlags);
        }
        restore();
    }
}

/// Undoes everything `TerminalMode::enter` did. Idempotent, and safe to call having entered nothing.
pub fn restore() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut out = std::io::stdout();
    let _ = execute!(
        out,
        DisableMouseCapture,
        DisableFocusChange,
        DisableBracketedPaste,
        cursor::Show,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
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
