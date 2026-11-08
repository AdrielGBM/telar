//! A simulated seat: one pointer and its data device, shared by every headless window attached to it, in one run or across runs on other threads, the way a compositor's seat is shared by every client.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

use platform_core::{
    DragOutcome, DropAnswer, Event, OutgoingDrag, PointerButton, PointerSource, SurfaceId,
};

/// What the person at the seat does next, in logical coordinates of the surface named.
#[derive(Debug, Clone, PartialEq)]
pub enum PointerStep {
    Press {
        surface: SurfaceId,
        x: f64,
        y: f64,
    },
    Move {
        surface: SurfaceId,
        x: f64,
        y: f64,
    },
    /// The pointer goes where no attached surface is, such as another program or the desktop.
    Leave,
    Release,
}

/// The seat every attached headless window shares. Steps run one at a time, each once every surface it reached has handled what it delivered, so a script reads the same however the runs driving those surfaces interleave.
#[derive(Clone, Default)]
pub struct HeadlessSeat {
    inner: Arc<Mutex<Seat>>,
}

#[derive(Default)]
struct Seat {
    steps: VecDeque<PointerStep>,
    delivered: HashMap<SurfaceId, Vec<Event>>,
    unsettled: HashSet<SurfaceId>,
    pointer: Option<(SurfaceId, f64, f64)>,
    held: bool,
    drag: Option<Drag>,
}

struct Drag {
    source: SurfaceId,
    outgoing: OutgoingDrag,
    over: Option<SurfaceId>,
    answers: HashMap<SurfaceId, Option<DropAnswer>>,
    icon_shown: bool,
}

impl HeadlessSeat {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn script(&self, steps: impl IntoIterator<Item = PointerStep>) {
        self.lock().steps.extend(steps);
    }

    /// Whether a scripted step or an event the seat delivered is still waiting for a surface.
    pub fn is_busy(&self) -> bool {
        let seat = self.lock();
        !seat.steps.is_empty() || !seat.unsettled.is_empty()
    }

    pub fn is_dragging(&self) -> bool {
        self.lock().drag.is_some()
    }

    /// Whether the picture following the drag in flight shows; `None` with no drag.
    pub fn drag_icon_shown(&self) -> Option<bool> {
        self.lock().drag.as_ref().map(|drag| drag.icon_shown)
    }

    /// What the seat has for `surface`. Asking when nothing is waiting says the surface handled the last batch, which is what lets the script move on.
    pub fn take_events(&self, surface: SurfaceId) -> Vec<Event> {
        let mut seat = self.lock();
        if let Some(events) = seat.delivered.remove(&surface) {
            return events;
        }
        seat.unsettled.remove(&surface);
        while seat.unsettled.is_empty()
            && let Some(step) = seat.steps.pop_front()
        {
            seat.run(step);
        }
        seat.delivered.remove(&surface).unwrap_or_default()
    }

    pub(crate) fn start_drag(&self, surface: SurfaceId, outgoing: OutgoingDrag) -> bool {
        let mut seat = self.lock();
        let Some((under, x, y)) = seat.pointer else {
            return false;
        };
        if !seat.held || under != surface || seat.drag.is_some() || outgoing.data.is_empty() {
            return false;
        }
        seat.drag = Some(Drag {
            source: surface,
            outgoing,
            over: None,
            answers: HashMap::new(),
            icon_shown: true,
        });
        seat.drag_over(surface, x, y);
        true
    }

    pub(crate) fn answer_drag(&self, surface: SurfaceId, answer: Option<DropAnswer>) {
        if let Some(drag) = self.lock().drag.as_mut() {
            drag.answers.insert(surface, answer);
        }
    }

    pub(crate) fn show_drag_icon(&self, shown: bool) {
        if let Some(drag) = self.lock().drag.as_mut() {
            drag.icon_shown = shown;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Seat> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Seat {
    fn deliver(&mut self, surface: SurfaceId, event: Event) {
        self.delivered.entry(surface).or_default().push(event);
        self.unsettled.insert(surface);
    }

    fn run(&mut self, step: PointerStep) {
        match step {
            PointerStep::Press { surface, x, y } => {
                self.pointer = Some((surface, x, y));
                self.held = true;
                let source = PointerSource::Mouse;
                self.deliver(
                    surface,
                    Event::PointerMoved {
                        x,
                        y,
                        source: source.clone(),
                    },
                );
                self.deliver(
                    surface,
                    Event::PointerPressed {
                        x,
                        y,
                        button: PointerButton::Primary,
                        source,
                    },
                );
            }
            PointerStep::Move { surface, x, y } => {
                let left = self
                    .pointer
                    .map(|(from, ..)| from)
                    .filter(|&from| from != surface);
                self.pointer = Some((surface, x, y));
                if self.drag.is_some() {
                    self.drag_over(surface, x, y);
                    return;
                }
                if let Some(from) = left {
                    self.deliver(from, Event::CursorLeft);
                }
                self.deliver(
                    surface,
                    Event::PointerMoved {
                        x,
                        y,
                        source: PointerSource::Mouse,
                    },
                );
            }
            PointerStep::Leave => {
                let left = self.pointer.take().map(|(from, ..)| from);
                let left = match self.drag.as_mut() {
                    Some(drag) => drag.over.take().map(|over| (over, Event::DragLeft)),
                    None => left.map(|from| (from, Event::CursorLeft)),
                };
                if let Some((surface, event)) = left {
                    self.deliver(surface, event);
                }
            }
            PointerStep::Release => {
                self.held = false;
                match self.drag.take() {
                    Some(drag) => self.drop_here(drag),
                    None => {
                        if let Some((surface, x, y)) = self.pointer {
                            self.deliver(
                                surface,
                                Event::PointerReleased {
                                    x,
                                    y,
                                    button: PointerButton::Primary,
                                    source: PointerSource::Mouse,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    fn drag_over(&mut self, surface: SurfaceId, x: f64, y: f64) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        let left = drag.over.replace(surface).filter(|&over| over != surface);
        let offer = drag.outgoing.offer();
        if let Some(over) = left {
            self.deliver(over, Event::DragLeft);
        }
        self.deliver(surface, Event::DragOver { x, y, offer });
    }

    fn drop_here(&mut self, drag: Drag) {
        let target = drag.over.zip(self.pointer);
        let delivered = target.and_then(|(over, (_, x, y))| {
            let answer = drag.answers.get(&over).cloned().flatten()?;
            let data = drag.outgoing.deliver(&answer)?;
            Some((over, x, y, data))
        });
        let outcome = match delivered {
            Some((over, x, y, data)) => {
                let action = data.action;
                self.deliver(over, Event::Dropped { x, y, data });
                DragOutcome::Dropped(action)
            }
            None => {
                if let Some((over, _)) = target {
                    self.deliver(over, Event::DragLeft);
                }
                DragOutcome::Cancelled
            }
        };
        self.deliver(drag.source, Event::DragEnded { outcome });
    }
}

#[cfg(test)]
#[path = "seat.test.rs"]
mod tests;
