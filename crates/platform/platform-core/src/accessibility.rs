//! What the UI tells the platform about itself, for whatever is listening on the other side.
//!
//! Here rather than in the UI layer for the same reason [`Event`](crate::Event) is: it is the vocabulary the
//! two ends share. The UI declares roles and names; the platform's accessibility API — AccessKit on the
//! desktop — is what turns them into something a screen reader can read. Neither end owns the words.

use geometry_core::Rect;

/// The vocabulary itself lives a layer down, where a renderer can reach it too: the desktop announcing a
/// checkbox and a document drawing one have to be describing the same box.
pub use semantics_core::Role;

/// One thing a screen reader can land on.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessNode {
    /// A stable identity within this window for as long as the widget lives. `None` for text that is not a
    /// control, which nothing needs to address.
    pub id: Option<u64>,
    pub role: Role,
    pub name: String,
    /// Window-absolute, which is the space every platform accessibility layer works in.
    pub rect: Rect,
    pub focused: bool,
    /// `false` announces "unavailable"; a control that is genuinely not there is absent instead.
    pub enabled: bool,
    /// Whether a control that carries a checked state is in it. `None` for the roles that have no such state —
    /// and never a default of `false` for the ones that do, which would announce every checkbox as unticked.
    pub toggled: Option<bool>,
    /// Where a control that carries a number stands, and between which bounds.
    ///
    /// Without it a slider announces "Volume, slider" and stops — the reader can say what the control is and
    /// not what it says, which is the one thing a value control exists to report. `None` for the roles that
    /// carry no number.
    pub value: Option<NumericValue>,
}

/// A numeric control's reading: where it is now, and the range that makes that number mean something.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericValue {
    pub now: f64,
    pub min: f64,
    pub max: f64,
}
