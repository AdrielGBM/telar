//! Putting the terminal into the state an application draws on, and — whatever happens — putting it back.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::style::Print;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{Command, QueueableCommand, cursor};

// XTWINOPS: push the icon name and window title on entry and pop them on the way out, so the titles the app sets leave with it. A terminal without a title stack ignores both.
const SAVE_TITLE: &str = "\x1b[22;0t";
const RESTORE_TITLE: &str = "\x1b[23;0t";

/// What a session put the terminal into. Shared with the panic hook, which has no other way to know whether there is anything to undo: whichever of the two leaves first undoes exactly what entering did, and the other finds nothing — undoing nothing must be harmless, because the hook runs on every panic in the process.
pub struct Entered {
    active: AtomicBool,
    mouse: AtomicBool,
    keyboard_enhanced: AtomicBool,
}

impl Entered {
    pub const fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            mouse: AtomicBool::new(false),
            keyboard_enhanced: AtomicBool::new(false),
        }
    }
}

static ENTERED: Entered = Entered::new();

/// The terminal a session drives: its raw-mode switch, whether it speaks the kitty keyboard protocol, and the commands that enter application mode and leave it.
///
/// A session hands over commands rather than bytes because crossterm carries some of them out as console calls on Windows — mouse capture is the console's input mode there, and the kitty protocol does not exist — so entering and leaving run against a recording exactly as they run against the real terminal, on every platform.
pub trait Terminal {
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
    fn supports_keyboard_enhancement(&mut self) -> bool;
    fn queue(&mut self, command: impl Command) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
}

pub struct CrosstermTerminal(io::Stdout);

impl Terminal for CrosstermTerminal {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        crossterm::terminal::enable_raw_mode()
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        crossterm::terminal::disable_raw_mode()
    }

    fn supports_keyboard_enhancement(&mut self) -> bool {
        crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false)
    }

    fn queue(&mut self, command: impl Command) -> io::Result<()> {
        self.0.queue(command).map(drop)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Owns the terminal's application mode. Restoring is done by `Drop` **and** by a panic hook, because a panic that unwinds past the drop — or one in a thread that aborts the process — would otherwise leave the user with a terminal in raw mode, no cursor, and no echo: a shell they cannot type into.
pub struct Session<'a, T: Terminal> {
    entered: &'a Entered,
    terminal: T,
}

pub type TerminalMode = Session<'static, CrosstermTerminal>;

impl TerminalMode {
    pub fn enter(mouse: bool) -> io::Result<Self> {
        install_panic_hook();
        Session::enter_with(&ENTERED, CrosstermTerminal(io::stdout()), mouse)
    }
}

impl<'a, T: Terminal> Session<'a, T> {
    pub fn enter_with(entered: &'a Entered, mut terminal: T, mouse: bool) -> io::Result<Self> {
        terminal.enable_raw_mode()?;
        entered.active.store(true, Ordering::SeqCst);
        let mut session = Self { entered, terminal };
        session.begin(mouse)?;
        Ok(session)
    }

    fn begin(&mut self, mouse: bool) -> io::Result<()> {
        let terminal = &mut self.terminal;
        terminal.queue(EnterAlternateScreen)?;
        terminal.queue(cursor::Hide)?;
        terminal.queue(EnableBracketedPaste)?;
        terminal.queue(EnableFocusChange)?;
        terminal.queue(Print(SAVE_TITLE))?;
        if mouse {
            self.entered.mouse.store(true, Ordering::SeqCst);
            terminal.queue(EnableMouseCapture)?;
        }
        terminal.flush()?;
        // Key *release* and unambiguous modifier reporting only exist under the kitty keyboard protocol. Without it a terminal reports presses alone, which is why the event mapping cannot assume releases.
        if terminal.supports_keyboard_enhancement() {
            self.entered.keyboard_enhanced.store(true, Ordering::SeqCst);
            terminal.queue(PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES,
            ))?;
        }
        terminal.flush()
    }

    /// Whether the terminal reports key releases. A backend that assumes it does on a terminal that does not leaves every key stuck down.
    pub fn reports_key_releases(&self) -> bool {
        self.entered.keyboard_enhanced.load(Ordering::SeqCst)
    }
}

impl<T: Terminal> Drop for Session<'_, T> {
    fn drop(&mut self) {
        leave(self.entered, &mut self.terminal);
    }
}

/// Undoes everything `TerminalMode::enter` did. Idempotent, and safe to call having entered nothing.
pub fn restore() {
    leave(&ENTERED, &mut CrosstermTerminal(io::stdout()));
}

// Every step is tried whatever became of the one before: a terminal left on the alternate screen because releasing the mouse failed is as unusable as one left raw.
fn leave(entered: &Entered, terminal: &mut impl Terminal) {
    if !entered.active.swap(false, Ordering::SeqCst) {
        return;
    }
    if entered.keyboard_enhanced.swap(false, Ordering::SeqCst) {
        let _ = terminal.queue(PopKeyboardEnhancementFlags);
    }
    if entered.mouse.swap(false, Ordering::SeqCst) {
        let _ = terminal.queue(DisableMouseCapture);
    }
    let _ = terminal.queue(DisableFocusChange);
    let _ = terminal.queue(DisableBracketedPaste);
    let _ = terminal.queue(cursor::Show);
    let _ = terminal.queue(LeaveAlternateScreen);
    let _ = terminal.queue(Print(RESTORE_TITLE));
    let _ = terminal.flush();
    let _ = terminal.disable_raw_mode();
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
