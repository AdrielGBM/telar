//! Runs a [`Play`] on a canvas once it is safe to drive: after the canvas first has a size and every write before it has settled, so each step sees what the one before it changed.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use telar::preview::Play;
use telar::{Timer, after_settle, effect, on_cleanup, run_after};

/// Hands `play` to `run` on the turn of the loop after its canvas first has a size and once the writes pending then have settled; dropped unrun with the owner this is called in.
pub(crate) fn run_when_sized(play: Play, run: impl FnOnce(Play) + 'static) {
    let canvas = Rc::downgrade(play.canvas());
    let waiting = RefCell::new(Some((play, run)));
    let timer: Rc<RefCell<Option<Timer>>> = Rc::default();
    let scheduled = Rc::clone(&timer);
    effect(move || {
        let sized = canvas.upgrade().is_some_and(|canvas| {
            let size = canvas.size();
            size.width > 0.0 && size.height > 0.0
        });
        if !sized {
            return;
        }
        let Some((play, run)) = waiting.borrow_mut().take() else {
            return;
        };
        // A timer runs inside a batch, where a step's writes would not reach the canvas before the next step reads it.
        let timed = run_after(Duration::ZERO, move || after_settle(move || run(play)));
        scheduled.replace(Some(timed));
    });
    on_cleanup(move || drop(timer.take()));
}
