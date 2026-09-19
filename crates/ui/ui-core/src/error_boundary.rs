//! [`ErrorBoundary`]: a subtree whose failure is replaced by a fallback rather than taking its surface down.

use std::cell::RefCell;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::{Rc, Weak};

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::{
    OwnerId, PanicCatch, PanicPayload, RwSignal, SurfaceHandle, current_owner, current_surface,
    dispose_owner, owner_scope, set_panic_catch, signal, with_owner,
};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{new_container, record_nodes, set_children, track_layout};
use crate::layout_item::{Child, LayoutItem, dispose_child, make_child};
use crate::pointer::dispatch_container_event;

/// Why a boundary's subtree was replaced by its fallback.
#[derive(Debug)]
pub enum BuildFailure {
    /// The build returned an error.
    Failed(LayoutError),
    /// The build, or an effect of the subtree it built, panicked. Holds the panic message.
    Panicked(String),
}

impl fmt::Display for BuildFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Failed(err) => write!(f, "build failed: {err}"),
            Self::Panicked(message) => write!(f, "build panicked: {message}"),
        }
    }
}

impl std::error::Error for BuildFailure {}

type Build = Box<dyn FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>>;
type Fallback = Box<dyn FnOnce(BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError>>;

struct BoundaryState {
    node: NodeId,
    child: Option<Child>,
    /// Taken by the first failure, so a subtree fails over once and its later panics are already answered.
    fallback: Option<Fallback>,
}

/// Builds a subtree in its own reactive scope and mounts `fallback` in its place when the subtree fails: an `Err` from the build, or a panic during the build or any later effect it owns; nested boundaries catch at the innermost, and disposal waits for an event dispatch already in progress.
pub struct ErrorBoundary {
    node: NodeId,
    rect: RwSignal<Rect>,
    state: Rc<RefCell<BoundaryState>>,
    version: RwSignal<u64>,
}

enum Attempt {
    Failed(LayoutError),
    Panicked(PanicPayload),
}

impl ErrorBoundary {
    /// A boundary laid out as a column around whichever subtree it shows.
    pub fn new(
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
        fallback: impl FnOnce(BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    ) -> Result<Self, LayoutError> {
        Self::with_style(LayoutStyle::new().flex_column(), build, fallback)
    }

    /// [`new`](Self::new) with the boundary's own node laid out by `style`. Returns the fallback's error when the build and then the fallback fail, and resumes the fallback's panic when it panics, so an outer boundary answers for both.
    pub fn with_style(
        style: LayoutStyle,
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
        fallback: impl FnOnce(BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    ) -> Result<Self, LayoutError> {
        let node = new_container(style, &[])?;
        let rect = track_layout(node).expect("boundary container is registered");
        let version = signal(0u64);
        let state = Rc::new(RefCell::new(BoundaryState {
            node,
            child: None,
            fallback: Some(Box::new(fallback)),
        }));

        let child = match attempt(Box::new(build)) {
            Ok((child, scope)) => {
                let catch = catch_for(
                    Rc::downgrade(&state),
                    version,
                    current_owner(),
                    current_surface(),
                );
                set_panic_catch(scope, Some(catch));
                child
            }
            Err(failure) => {
                let fallback = state.borrow_mut().fallback.take();
                let fallback = fallback.expect("nothing takes the fallback before the build fails");
                let failure = BuildFailure::from(failure);
                match attempt(Box::new(move || fallback(failure))) {
                    Ok((child, _)) => child,
                    Err(Attempt::Failed(err)) => return Err(err),
                    Err(Attempt::Panicked(payload)) => resume_unwind(payload),
                }
            }
        };
        set_children(node, &[child.node()])?;
        state.borrow_mut().child = Some(child);

        Ok(Self {
            node,
            rect,
            state,
            version,
        })
    }

    /// Whether the subtree failed and the fallback is what this boundary shows.
    pub fn has_failed(&self) -> bool {
        self.state.borrow().fallback.is_none()
    }
}

impl From<Attempt> for BuildFailure {
    fn from(attempt: Attempt) -> Self {
        match attempt {
            Attempt::Failed(err) => Self::Failed(err),
            Attempt::Panicked(payload) => Self::Panicked(panic_message(&payload)),
        }
    }
}

/// Runs `build` in a fresh scope that leaves its panics to this call, disposing the scope when the build fails.
fn attempt(build: Build) -> Result<(Child, OwnerId), Attempt> {
    let scope = owner_scope();
    let owner = scope.id();
    set_panic_catch(owner, Some(PanicCatch::Unwind));
    let nodes = record_nodes();
    let outcome = catch_unwind(AssertUnwindSafe(|| build().map(make_child)));
    drop(scope);
    match outcome {
        Ok(Ok(child)) => {
            nodes.keep();
            set_panic_catch(owner, None);
            Ok((child.owned_by(owner), owner))
        }
        Ok(Err(err)) => {
            dispose_owner(owner);
            drop(nodes);
            Err(Attempt::Failed(err))
        }
        Err(payload) => {
            dispose_owner(owner);
            drop(nodes);
            Err(Attempt::Panicked(payload))
        }
    }
}

/// What the built subtree's scope does with a panic from one of its re-runs: swap in the fallback, built where the boundary itself was.
fn catch_for(
    state: Weak<RefCell<BoundaryState>>,
    version: RwSignal<u64>,
    outer: Option<OwnerId>,
    surface: SurfaceHandle,
) -> PanicCatch {
    PanicCatch::Handle(Rc::new(move |payload| {
        let Some(state) = state.upgrade() else {
            return Ok(());
        };
        let Some(fallback) = state.borrow_mut().fallback.take() else {
            return Ok(());
        };
        let failure = BuildFailure::Panicked(panic_message(&payload));
        let replaced = {
            let _surface = surface.enter();
            with_owner(outer, || attempt(Box::new(move || fallback(failure))))
        };
        let (container, failed) = {
            let mut st = state.borrow_mut();
            (st.node, st.child.take())
        };
        let unanswered = match replaced {
            Ok((child, _)) => {
                let _ = set_children(container, &[child.node()]);
                state.borrow_mut().child = Some(child);
                None
            }
            Err(Attempt::Failed(err)) => {
                let _ = set_children(container, &[]);
                Some(Box::new(format!("error boundary fallback failed: {err}")) as PanicPayload)
            }
            Err(Attempt::Panicked(fallback_payload)) => {
                let _ = set_children(container, &[]);
                Some(fallback_payload)
            }
        };
        if let Some(failed) = failed {
            dispose_child(&failed);
        }
        version.update(|v| *v = v.wrapping_add(1));
        unanswered.map_or(Ok(()), Err)
    }))
}

fn panic_message(payload: &PanicPayload) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    payload
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_else(|| "a panic with a non-string payload".to_string())
}

impl LayoutItem for ErrorBoundary {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for ErrorBoundary {
    fn view(&self) -> RenderNode {
        self.version.get();
        let _ = self.rect.get();
        let st = self.state.borrow();
        let content = RenderNode::group(st.child.iter().map(|c| c.segment.boundary()));
        crate::element::wrap(self.node, content)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // A snapshot, because a handler inside may fail the subtree, and the swap writes this state while the dispatch is still under way.
        let mut children: Vec<Child> = self.state.borrow().child.iter().cloned().collect();
        dispatch_container_event(&mut children, event)
    }

    fn debug_name(&self) -> &'static str {
        "ErrorBoundary"
    }
}

#[cfg(test)]
#[path = "error_boundary_test.rs"]
mod tests;
