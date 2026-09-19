use layout_core::{LayoutError, LayoutStyle};
use reactive_core::{effect, signal};

use crate::container::Container;
use crate::layout_item::LayoutItem;

/// What a disposal is meant to free, counted, so a test can compare before and after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Live {
    signals: usize,
    effects: usize,
    owners: usize,
    nodes: usize,
}

pub(crate) fn live() -> Live {
    Live {
        signals: reactive_core::live_signal_count(),
        effects: reactive_core::live_effect_count(),
        owners: reactive_core::live_owner_count(),
        nodes: crate::context::live_node_count(),
    }
}

/// A leaf whose build creates reactive state of its own and a scope beneath it, then fails the way `fail` says.
pub(crate) fn stateful_leaf(fail: Failure) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let count = signal(0u32);
    effect(move || {
        count.get();
    });
    let _inner = reactive_core::owner_scope();
    signal(());
    let leaf = Container::new(LayoutStyle::new().width(10.0).height(10.0), vec![])?;
    match fail {
        Failure::None => Ok(Box::new(leaf)),
        Failure::Panic => panic!("build failed"),
        Failure::Err => Err(LayoutError::Engine("build failed".into())),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Failure {
    None,
    Panic,
    Err,
}

impl std::ops::Sub for Live {
    type Output = Live;

    fn sub(self, before: Live) -> Live {
        Live {
            signals: self.signals - before.signals,
            effects: self.effects - before.effects,
            owners: self.owners - before.owners,
            nodes: self.nodes - before.nodes,
        }
    }
}
