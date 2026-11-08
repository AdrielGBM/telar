//! A box as the start of an OS drag, and as a place a drag from anywhere can be dropped.

use platform_core::{
    DragOffer, DragOutcome, DropAction, DropAnswer, DroppedData, Event, MimeType, OutgoingDrag,
    PointerButton, TAP_SLOP, WindowCommand,
};

/// The types a drop target takes, in the order it prefers them, and the action it would rather a drop perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropTarget {
    accepts: Vec<MimeType>,
    prefers: DropAction,
}

impl DropTarget {
    pub fn new(accepts: impl IntoIterator<Item = MimeType>) -> Self {
        Self {
            accepts: accepts.into_iter().collect(),
            prefers: DropAction::Copy,
        }
    }

    pub fn preferring(mut self, action: DropAction) -> Self {
        self.prefers = action;
        self
    }

    pub fn answer(&self, offer: &DragOffer) -> Option<DropAnswer> {
        offer.first_of(&self.accepts).map(|mime| DropAnswer {
            mime: mime.clone(),
            action: offer.actions.choose(self.prefers),
        })
    }

    pub fn takes(&self, mime: &MimeType) -> bool {
        self.accepts.contains(mime)
    }
}

type OnDrop = Box<dyn Fn(&DroppedData, f32, f32)>;
type OnHover = Box<dyn Fn(Option<(f32, f32)>)>;

pub(crate) struct DropZone {
    target: DropTarget,
    on_drop: OnDrop,
    on_hover: OnHover,
    hovered: bool,
}

impl DropZone {
    pub(crate) fn new(
        target: DropTarget,
        on_hover: impl Fn(Option<(f32, f32)>) + 'static,
        on_drop: impl Fn(&DroppedData, f32, f32) + 'static,
    ) -> Self {
        Self {
            target,
            on_drop: Box::new(on_drop),
            on_hover: Box::new(on_hover),
            hovered: false,
        }
    }

    /// Answers a hover at `local` when it is this zone's to take, pushing the answer for the runner to hand the platform.
    pub(crate) fn over(&mut self, offer: &DragOffer, local: Option<(f32, f32)>) -> bool {
        let answer = local.and_then(|_| self.target.answer(offer));
        let taken = answer.is_some();
        if let Some(answer) = answer {
            platform_core::push_window_command(WindowCommand::AcceptDrop(answer));
        }
        self.hover(local.filter(|_| taken));
        taken
    }

    pub(crate) fn dropped(&mut self, data: &DroppedData, local: Option<(f32, f32)>) -> bool {
        self.hover(None);
        match local {
            Some((x, y)) if self.target.takes(&data.mime) => {
                (self.on_drop)(data, x, y);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn left(&mut self) {
        self.hover(None);
    }

    fn hover(&mut self, at: Option<(f32, f32)>) {
        let hovered = at.is_some();
        if !hovered && !self.hovered {
            return;
        }
        self.hovered = hovered;
        (self.on_hover)(at);
    }
}

pub(crate) struct DragSource {
    payload: Box<dyn Fn() -> Option<OutgoingDrag>>,
    on_ended: Box<dyn Fn(DragOutcome)>,
    origin: Option<(f32, f32)>,
    in_flight: bool,
}

impl DragSource {
    pub(crate) fn new(
        payload: impl Fn() -> Option<OutgoingDrag> + 'static,
        on_ended: impl Fn(DragOutcome) + 'static,
    ) -> Self {
        Self {
            payload: Box::new(payload),
            on_ended: Box::new(on_ended),
            origin: None,
            in_flight: false,
        }
    }

    pub(crate) fn press(&mut self, event: &Event, hit: geometry_core::Rect) -> bool {
        let Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            ..
        } = event
        else {
            return false;
        };
        if !hit.contains(*x as f32, *y as f32) {
            return false;
        }
        self.origin = Some((*x as f32, *y as f32));
        crate::drag::claim();
        true
    }

    /// Starts the drag once the stroke travels past the slop a tap is allowed, the same distance that cancels the tap, so a stroke is one or the other.
    pub(crate) fn moved(&mut self, event: &Event) -> bool {
        let (Some((ox, oy)), Event::PointerMoved { x, y, .. }) = (self.origin, event) else {
            return false;
        };
        if (*x as f32 - ox).hypot(*y as f32 - oy) <= TAP_SLOP {
            return false;
        }
        self.origin = None;
        let Some(drag) = (self.payload)() else {
            return false;
        };
        platform_core::push_window_command(WindowCommand::StartDrag(drag));
        self.in_flight = true;
        true
    }

    pub(crate) fn disarm(&mut self) {
        self.origin = None;
    }

    pub(crate) fn ended(&mut self, outcome: DragOutcome) {
        if !std::mem::take(&mut self.in_flight) {
            return;
        }
        (self.on_ended)(outcome);
    }
}

#[cfg(test)]
#[path = "drag_drop.test.rs"]
mod tests;
