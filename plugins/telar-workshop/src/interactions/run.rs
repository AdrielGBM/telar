//! What a run of a play came to, as the panel lists it: each step in order, with how it went and the frame it left.

use std::rc::Rc;

use telar::preview::{Frame, Play, PlayStep};

use crate::guarded::guarded;

/// The steps of one run of a preview's play on one canvas, or of the last run waiting to run again on a canvas mounted since.
#[derive(Clone)]
pub(crate) struct Run {
    pub(crate) entry: &'static str,
    /// Which mount of the canvas the steps belong to.
    pub(crate) mount: u64,
    pub(crate) steps: Rc<[Step]>,
    pub(crate) ran: bool,
}

#[derive(Clone)]
pub(crate) struct Step {
    /// What was done, as the play logged it: `click button "Save"`.
    pub(crate) action: String,
    pub(crate) status: StepStatus,
    /// The canvas as the step left it; `None` for a step waiting to run.
    pub(crate) frame: Option<Frame>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum StepStatus {
    Pending,
    Passed,
    Failed(String),
}

/// What a run came to as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Verdict {
    NotRun,
    Passed,
    /// The first step that failed, by index.
    Failed(usize),
}

impl Run {
    /// Runs `play`'s entry's play on its canvas, a panic in it failing a last step of its own, then takes its pointer off the canvas the person goes on to use.
    pub(crate) fn of(mount: u64, play: &mut Play) -> Self {
        let outcome = guarded(|| play.run());
        let mut steps: Vec<Step> = play.steps().iter().map(Step::of).collect();
        let _ = guarded(|| play.let_go());
        if let Err(panic) = outcome {
            steps.push(Step {
                action: String::from("play"),
                status: StepStatus::Failed(panic),
                frame: guarded(|| play.frame()).ok(),
            });
        }
        Self {
            entry: play.entry().id,
            mount,
            steps: steps.into(),
            ran: true,
        }
    }

    /// These steps waiting to run again on the canvas mounted as `mount`, with nothing to show of them yet.
    pub(crate) fn pending(&self, mount: u64) -> Self {
        let steps = self
            .steps
            .iter()
            .map(|step| Step {
                action: step.action.clone(),
                status: StepStatus::Pending,
                frame: None,
            })
            .collect();
        Self {
            entry: self.entry,
            mount,
            steps,
            ran: false,
        }
    }

    pub(crate) fn verdict(&self) -> Verdict {
        if !self.ran {
            return Verdict::NotRun;
        }
        match self.steps.iter().position(Step::failed) {
            Some(index) => Verdict::Failed(index),
            None => Verdict::Passed,
        }
    }
}

impl Step {
    fn of(step: &PlayStep) -> Self {
        Self {
            action: step.action.clone(),
            status: match &step.outcome {
                Ok(()) => StepStatus::Passed,
                Err(error) => StepStatus::Failed(error.message.clone()),
            },
            frame: Some(step.frame.clone()),
        }
    }

    pub(crate) fn failed(&self) -> bool {
        matches!(self.status, StepStatus::Failed(_))
    }
}
