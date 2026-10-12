//! What names a box in the document: its id, and the surface that numbered it.

use std::fmt;

use platform_core::Event;

/// A box as the document names it.
///
/// Each surface numbers its boxes from its own counter, so a surface nested in the frame ([`DrawCommand::PushSurface`](renderer_core::DrawCommand::PushSurface)) has boxes whose ids the window's boxes take too. The handle of the innermost surface around the box tells them apart; the window's own boxes have none, and are written as their bare id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct BoxKey {
    pub surface: Option<u64>,
    pub id: u64,
}

impl BoxKey {
    pub(crate) fn new(surface: Option<u64>, id: u64) -> Self {
        Self { surface, id }
    }

    /// The key as an attribute wrote it.
    pub(crate) fn parse(written: &str) -> Option<Self> {
        match written.split_once('-') {
            Some((surface, id)) => Some(Self::new(Some(surface.parse().ok()?), id.parse().ok()?)),
            None => Some(Self::new(None, written.parse().ok()?)),
        }
    }

    /// `event`, a box event in the terms of the surface that numbered this box, as the window hears it: passed on into that surface by the frames that draw it.
    pub(crate) fn event(self, event: Event) -> Event {
        match self.surface {
            Some(surface) => Event::InSurface {
                surface,
                event: Box::new(event),
            },
            None => event,
        }
    }
}

impl From<u64> for BoxKey {
    fn from(id: u64) -> Self {
        Self::new(None, id)
    }
}

/// Written so it can stand inside an element `id` and a definition's name as well as an attribute.
impl fmt::Display for BoxKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.surface {
            Some(surface) => write!(f, "{surface}-{}", self.id),
            None => write!(f, "{}", self.id),
        }
    }
}

#[cfg(test)]
#[path = "box_key_test.rs"]
mod tests;
