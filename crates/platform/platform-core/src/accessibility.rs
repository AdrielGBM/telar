//! What the UI tells the platform about itself, for whatever is listening on the other side.
//!
//! Here rather than in the UI layer for the same reason [`Event`](crate::Event) is: it is the vocabulary the
//! two ends share. The UI declares roles and names; the platform's accessibility API — AccessKit on the
//! desktop — is what turns them into something a screen reader can read. Neither end owns the words.

/// What a control *is*, for the reader that has to say it out loud.
///
/// A separate question from what a widget does with a key: both a checkbox and a button take keys as
/// commands, and only one of them is a thing you can be told the state of. The list is short on purpose —
/// these are the roles a real catalogue has, not a transcription of any platform's vocabulary — and each
/// backend maps them outwards to its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Role {
    /// The default, and right for anything whose whole meaning is "activating this does something".
    #[default]
    Button,
    /// Carries a checked state that is part of what it is, not of what it looks like.
    CheckBox,
    /// One of a set where choosing it unchooses the others.
    Radio,
    /// A checkbox that reads as a switch: on or off rather than ticked or not.
    Switch,
    /// Picks one of several panels.
    Tab,
    /// A row of a menu or a bound list.
    MenuItem,
    /// A continuous value dragged along a track.
    Slider,
    /// A discrete value with a step, typed or nudged.
    SpinButton,
    /// A single-line field.
    TextInput,
    /// A multi-line editor.
    MultilineTextInput,
    /// Opens a list of choices and names the current one.
    ComboBox,
    /// A region that reads as one thing and can be collapsed.
    Disclosure,
    /// Not a control at all: text the interface is showing. Never focusable — it is here because a reader
    /// given only the buttons cannot say what the buttons are for.
    Label,
}
