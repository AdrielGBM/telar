//! The process-wide sink a backend posts events into from a thread that is not the UI one.

use std::sync::{Arc, RwLock};

use crate::Event;

/// Where an event goes when the platform is not the one that noticed it.
type Sink = Arc<dyn Fn(Event) + Send + Sync>;

static EVENT_SINK: RwLock<Option<Sink>> = RwLock::new(None);

/// Registers the process-global event sink, installed once by the platform at startup, alongside the loop waker it is the counterpart of: that one asks for a frame, this one supplies what the frame should be about.
///
/// It exists because a renderer is not always only a renderer. A backend that draws pixels is write-only by nature — nothing it puts on a surface can report back. A backend whose output is a *document* creates real elements, and real elements notice things nothing else can: a composition an input method finished, a value the browser autofilled, a region the user scrolled with two fingers on a trackpad. Those are events in the platform's own vocabulary, and without a way in they are simply lost.
///
/// The sink must queue rather than dispatch: it is called from inside the platform's own callbacks and from inside a frame, and re-entering the running handler from either is a panic rather than a nested dispatch.
pub fn set_event_sink(sink: Sink) {
    *EVENT_SINK.write().unwrap() = Some(sink);
}

/// Hands `event` to the platform as though it had noticed it.
///
/// Silently dropped where no platform installed a sink, which is every backend that has nothing to report: there is no queue to put it in and nobody to read it.
pub fn post_event(event: Event) {
    let sink = EVENT_SINK.read().unwrap().clone();
    if let Some(sink) = sink {
        sink(event);
    }
}
