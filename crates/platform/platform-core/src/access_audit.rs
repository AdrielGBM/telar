//! What checking a surface against what its reader is told finds: the rules, how much each matters, and the findings themselves.
//!
//! Here beside [`AccessNode`] rather than with the checks, because the findings travel where the checks cannot: from a hot-reloaded application, whose registries the checks read, to the devtools overlay in the host that shows them.

use std::fmt;

use geometry_core::Rect;

use crate::AccessNode;

/// How much a finding matters: an [`Error`](Self::Error) keeps someone from using the surface, a [`Warning`](Self::Warning) makes it harder or depends on where the surface ends up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AccessSeverity {
    Warning,
    Error,
}

impl AccessSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            AccessSeverity::Warning => "warning",
            AccessSeverity::Error => "error",
        }
    }
}

/// What a finding is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccessRule {
    /// A control a reader announces by its role alone.
    UnnamedControl,
    /// A control activated by pressing it that is named only by symbols, which a reader spells out or skips.
    SymbolOnlyName,
    /// A picture a reader is not told about, and not told to skip.
    UnnamedPicture,
    /// Text too close in luminance to the nearest opaque fill behind it.
    Contrast,
    /// A pointer target too small with something else within reach of it.
    TargetSize,
    /// A Tab stop a reader is never told is there.
    HiddenFocusable,
    /// Tab moving back against the reading order.
    FocusOrder,
    /// Text in a surface that names no language.
    MissingLang,
}

impl AccessRule {
    pub const ALL: [AccessRule; 8] = [
        AccessRule::UnnamedControl,
        AccessRule::SymbolOnlyName,
        AccessRule::UnnamedPicture,
        AccessRule::Contrast,
        AccessRule::TargetSize,
        AccessRule::HiddenFocusable,
        AccessRule::FocusOrder,
        AccessRule::MissingLang,
    ];

    /// The rule's name in a report and on a command line.
    pub const fn id(self) -> &'static str {
        match self {
            AccessRule::UnnamedControl => "unnamed-control",
            AccessRule::SymbolOnlyName => "symbol-only-name",
            AccessRule::UnnamedPicture => "unnamed-picture",
            AccessRule::Contrast => "contrast",
            AccessRule::TargetSize => "target-size",
            AccessRule::HiddenFocusable => "hidden-focusable",
            AccessRule::FocusOrder => "focus-order",
            AccessRule::MissingLang => "missing-lang",
        }
    }

    /// Target size and focus order are judged by geometry that can be right for reasons the frame does not show, and the language is often the host's to give rather than the surface's, so those three warn.
    pub const fn severity(self) -> AccessSeverity {
        match self {
            AccessRule::UnnamedControl
            | AccessRule::SymbolOnlyName
            | AccessRule::UnnamedPicture
            | AccessRule::Contrast
            | AccessRule::HiddenFocusable => AccessSeverity::Error,
            AccessRule::TargetSize | AccessRule::FocusOrder | AccessRule::MissingLang => {
                AccessSeverity::Warning
            }
        }
    }

    pub fn from_id(id: &str) -> Option<AccessRule> {
        AccessRule::ALL.into_iter().find(|rule| rule.id() == id)
    }
}

impl fmt::Display for AccessRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// One finding: the rule broken, how much it matters, what is wrong in words, and where, in the coordinates of the surface that was checked.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct AccessViolation {
    pub rule: AccessRule,
    pub severity: AccessSeverity,
    pub message: String,
    pub rect: Option<Rect>,
}

impl AccessViolation {
    pub fn new(rule: AccessRule, message: impl Into<String>, rect: Option<Rect>) -> Self {
        Self {
            rule,
            severity: rule.severity(),
            message: message.into(),
            rect,
        }
    }

    /// The same finding at `rect`, for a surface drawn somewhere else than where it was checked.
    pub fn at(self, rect: Option<Rect>) -> Self {
        Self { rect, ..self }
    }
}

impl fmt::Display for AccessViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:<7}  {:<16}  {}",
            self.severity.as_str(),
            self.rule.id(),
            self.message
        )
    }
}

/// A surface as a reader meets it, with what checking it found: the nodes a reader is told, in reading order, and the violations, both in the surface's coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AccessAudit {
    pub nodes: Vec<AccessNode>,
    pub violations: Vec<AccessViolation>,
}
