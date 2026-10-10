//! The interactions panel: the steps the selected preview's play took on the canvas, in order, each with how it went and the frame it left behind. The step picked is shown read-only in place of the live canvas until the panel goes back to live; a replay mounts the canvas afresh and runs the play again, and the play runs as each canvas mounts unless that is turned off.
//!
//! The play drives the canvas the workshop shows, through [`Play::attach`], so the canvas ends where the play left it and the actions panel lists what the preview's callbacks received on the way. It runs on the turn of the loop after the canvas first has a size, once every write before it has settled, so each step sees what the one before it changed.

mod frame;
mod run;
mod steps;

use std::cell::Cell;
use std::rc::Rc;

use telar::preview::{CaughtFailures, Frame, Play, PreviewCtx, PreviewEntry};
use telar::{RwSignal, SurfaceCanvas, batch, on_cleanup, signal};

pub(crate) use frame::frame_layer;
pub(crate) use run::{Run, Step, StepStatus, Verdict};
pub(crate) use steps::{panel, tab_badge, tools};

use crate::settle::run_when_sized;
use crate::state::{ViewMode, WorkshopState};

/// The play of the canvas the workshop shows: whether it runs as the canvas mounts, the steps of its last run, and the step the canvas shows in place of the live preview.
///
/// A cheap handle: clones share one state.
#[derive(Clone)]
pub(crate) struct Interactions {
    auto_run: RwSignal<bool>,
    run: RwSignal<Option<Run>>,
    picked: RwSignal<Option<usize>>,
    mounted: RwSignal<Option<&'static str>>,
    replaying: Rc<Cell<bool>>,
    mounts: Rc<Cell<u64>>,
}

/// The step the canvas shows in place of the live preview, and what the bar over it says.
#[derive(Clone)]
pub(crate) struct Shown {
    pub(crate) index: usize,
    pub(crate) total: usize,
    pub(crate) step: Step,
    pub(crate) frame: Frame,
}

impl Interactions {
    /// `auto_run` is whether a play runs as its canvas mounts: kept by the caller, since it outlives a run of the workshop.
    pub(crate) fn new(auto_run: RwSignal<bool>) -> Self {
        Self {
            auto_run,
            run: signal(None),
            picked: signal(None),
            mounted: signal(None),
            replaying: Rc::default(),
            mounts: Rc::default(),
        }
    }

    pub(crate) fn auto_run(&self) -> RwSignal<bool> {
        self.auto_run
    }

    /// The last run of the play of the preview the canvas shows, or its steps waiting to run again on a canvas mounted since. `None` before the preview's play first runs.
    pub(crate) fn run(&self) -> Option<Run> {
        self.run.get()
    }

    /// The preview the canvas shows, by id, while one is mounted there.
    pub(crate) fn mounted(&self) -> Option<&'static str> {
        self.mounted.get()
    }

    /// The step, by index, whose frame the canvas shows in place of the live preview.
    pub(crate) fn picked(&self) -> Option<usize> {
        self.picked.get()
    }

    /// Shows the frame step `index` left in place of the live preview; a step with no frame, or no step, goes back to live.
    pub(crate) fn pick(&self, index: Option<usize>) {
        let framed = index.filter(|&index| {
            self.run.with(|run| {
                run.as_ref()
                    .and_then(|run| run.steps.get(index))
                    .is_some_and(|step| step.frame.is_some())
            })
        });
        self.picked.set_if_changed(framed);
    }

    pub(crate) fn back_to_live(&self) {
        self.picked.set_if_changed(None);
    }

    /// The picked step, while the canvas shows its frame.
    pub(crate) fn shown(&self) -> Option<Shown> {
        let index = self.picked.get()?;
        self.run.with(|run| {
            let run = run.as_ref()?;
            let step = run.steps.get(index)?;
            Some(Shown {
                index,
                total: run.steps.len(),
                frame: step.frame.clone()?,
                step: step.clone(),
            })
        })
    }

    /// Which step the canvas shows, as a key that changes when the step or its run does.
    pub(crate) fn shown_key(&self) -> Option<(u64, usize)> {
        let index = self.picked.get()?;
        self.run
            .with(|run| run.as_ref().map(|run| (run.mount, index)))
    }

    /// Notes a canvas mounted for `entry`, and answers which mount it is: the steps of its play's last run wait to run again, and no step is picked.
    fn mounted_now(&self, entry: &'static str) -> u64 {
        let mount = self.mounts.get().wrapping_add(1);
        self.mounts.set(mount);
        batch(|| {
            self.mounted.set(Some(entry));
            self.picked.set_if_changed(None);
            self.run.update(|run| {
                *run = run
                    .take()
                    .filter(|run| run.entry == entry)
                    .map(|run| run.pending(mount));
            });
        });
        mount
    }

    fn unmounted(&self, mount: u64) {
        if self.mounts.get() == mount {
            batch(|| {
                self.mounted.set(None);
                self.picked.set_if_changed(None);
            });
        }
    }

    /// Keeps what `play` came to, unless its canvas has been replaced since.
    fn record(&self, mount: u64, mut play: Play) {
        if self.mounts.get() != mount {
            return;
        }
        let run = Run::of(mount, &mut play);
        batch(|| {
            self.picked.set_if_changed(None);
            self.run.set(Some(run));
        });
    }
}

/// Mounts the selected preview afresh on the canvas and runs its play there, whether or not plays run as their canvas mounts.
pub(crate) fn replay(state: &WorkshopState) {
    state.interactions().replaying.set(true);
    batch(|| {
        state.view().set_if_changed(ViewMode::Canvas);
        state.remount();
    });
}

/// Hands the canvas just mounted for `entry` against `ctx` to its play, which runs there once the canvas has a size if plays run as their canvas mounts or a replay asked for it. `failures` is what the canvas's error boundary records into.
pub(crate) fn drive(
    state: &WorkshopState,
    entry: &PreviewEntry,
    ctx: PreviewCtx,
    canvas: &Rc<SurfaceCanvas>,
    failures: CaughtFailures,
) {
    let interactions = state.interactions().clone();
    let mount = interactions.mounted_now(entry.id);
    let replaying = interactions.replaying.replace(false);
    let ending = interactions.clone();
    on_cleanup(move || ending.unmounted(mount));
    if entry.play.is_none() || !(replaying || interactions.auto_run.peek()) {
        return;
    }
    let play = Play::attach(entry, ctx, Rc::clone(canvas), failures);
    run_when_sized(play, move |play| interactions.record(mount, play));
}

#[cfg(test)]
#[path = "interactions_test.rs"]
mod tests;
