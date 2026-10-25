//! What the inspection panels read, and the one model both tools keep it in.
//!
//! An [`InspectSource`] is something drawn that can be inspected: the whole window the overlay draws over ([`WindowSource`]), or one canvas a workshop shows ([`SurfaceCanvas`], or the [`SurfaceFrame`] showing it). An [`Inspection`] is what the panels show of one, as signals, refreshed from it with [`Inspection::refresh`] or kept refreshed with [`Inspection::follow`]. Both tools read their sources through the same audit and the same walk, so where they show the same components they agree.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use telar::{
    AccessAudit, AccessNode, AccessViolation, Insets, Rect, RelayoutHook, RwSignal,
    SegmentNodeInfo, Size, SurfaceCanvas, SurfaceFrame, Transform, on_relayout, signal,
};

/// Something drawn that the inspection panels can read.
pub trait InspectSource {
    /// Bumped whenever what the source drew may have changed; two equal reads mean the same frame, so its walk and its audit are the same.
    fn generation(&self) -> u64;

    /// The mounted components in pre-order, each with the box it draws and its box model, in the source's coordinates.
    fn walk(&self) -> Vec<SegmentNodeInfo>;

    /// What a reader is told about the source and what the accessibility checks find, in its coordinates; `None` where it cannot be read, which the panels show as nothing to show.
    fn audit(&self) -> Option<AccessAudit>;

    /// Where the panels' marks for a rect of the source are drawn: maps the source's coordinates into those of the tree the panels are built in.
    fn placement(&self) -> Transform;
}

impl<T: InspectSource + ?Sized> InspectSource for Rc<T> {
    fn generation(&self) -> u64 {
        (**self).generation()
    }

    fn walk(&self) -> Vec<SegmentNodeInfo> {
        (**self).walk()
    }

    fn audit(&self) -> Option<AccessAudit> {
        (**self).audit()
    }

    fn placement(&self) -> Transform {
        (**self).placement()
    }
}

/// A canvas, read in its own world and placed where the frame showing it put it.
impl InspectSource for SurfaceCanvas {
    fn generation(&self) -> u64 {
        drop(self.frame_commands());
        SurfaceCanvas::generation(self)
    }

    fn walk(&self) -> Vec<SegmentNodeInfo> {
        let mut nodes = Vec::new();
        SurfaceCanvas::walk(self, &mut nodes);
        nodes
    }

    fn audit(&self) -> Option<AccessAudit> {
        let commands = self.frame_commands().clone();
        self.with_entered(|| Some(telar::inspect::audit(&commands, None)))
    }

    fn placement(&self) -> Transform {
        SurfaceCanvas::placement(self)
    }
}

impl InspectSource for SurfaceFrame {
    fn generation(&self) -> u64 {
        InspectSource::generation(&**self.canvas())
    }

    fn walk(&self) -> Vec<SegmentNodeInfo> {
        InspectSource::walk(&**self.canvas())
    }

    fn audit(&self) -> Option<AccessAudit> {
        InspectSource::audit(&**self.canvas())
    }

    fn placement(&self) -> Transform {
        InspectSource::placement(&**self.canvas())
    }
}

/// The whole window under the overlay, as the runner reports it each frame: the walk through `DevOverlay::on_tree`, and what a reader is told through `on_access` and `on_violations`, read in the application's runtime — across the reload boundary when the application is a hot-reloaded library. Drawn in window coordinates, which are the overlay's own.
#[derive(Default)]
pub struct WindowSource {
    generation: u64,
    nodes: Vec<SegmentNodeInfo>,
    audit: Option<AccessAudit>,
}

impl WindowSource {
    pub fn set_nodes(&mut self, nodes: &[SegmentNodeInfo]) {
        self.nodes.clear();
        self.nodes.extend_from_slice(nodes);
        self.generation += 1;
    }

    pub fn set_access(&mut self, nodes: &[AccessNode]) {
        let audit = self.audit.get_or_insert_with(AccessAudit::default);
        audit.nodes = nodes.to_vec();
        self.generation += 1;
    }

    pub fn set_violations(&mut self, violations: &[AccessViolation]) {
        let audit = self.audit.get_or_insert_with(AccessAudit::default);
        audit.violations = violations.to_vec();
        self.generation += 1;
    }
}

impl InspectSource for WindowSource {
    fn generation(&self) -> u64 {
        self.generation
    }

    fn walk(&self) -> Vec<SegmentNodeInfo> {
        self.nodes.clone()
    }

    fn audit(&self) -> Option<AccessAudit> {
        self.audit.clone()
    }

    fn placement(&self) -> Transform {
        Transform::IDENTITY
    }
}

/// One mounted component as the panels keep it: a [`SegmentNodeInfo`] whose name the panels own, so a walk read from a library that has since been unloaded leaves nothing dangling behind.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectedNode {
    pub id: u64,
    pub name: Arc<str>,
    pub depth: usize,
    pub rect: Rect,
    pub padding: Insets,
    pub margin: Insets,
    pub border: Insets,
    pub gap: Size,
}

impl InspectedNode {
    fn of(node: &SegmentNodeInfo) -> Self {
        Self {
            id: node.id,
            name: Arc::from(&*node.name),
            depth: node.depth,
            rect: node.rect,
            padding: node.padding,
            margin: node.margin,
            border: node.border,
            gap: node.gap,
        }
    }

    fn reads_as(&self, node: &SegmentNodeInfo) -> bool {
        self.id == node.id
            && *self.name == *node.name
            && self.depth == node.depth
            && self.rect == node.rect
            && self.padding == node.padding
            && self.margin == node.margin
            && self.border == node.border
            && self.gap == node.gap
    }
}

/// What the panels show of one source and what they let a person choose in it, as signals owned by the tree the panels are built in. A cheap `Copy` handle.
#[derive(Clone, Copy)]
pub struct Inspection {
    pub(crate) nodes: RwSignal<Rc<[InspectedNode]>>,
    pub(crate) audit: RwSignal<Option<Rc<AccessAudit>>>,
    pub(crate) placement: RwSignal<Transform>,
    /// The selected component, by the id the tree names it by.
    pub(crate) selected: RwSignal<Option<Arc<str>>>,
    pub(crate) expanded: RwSignal<HashSet<Arc<str>>>,
    seen_branches: RwSignal<HashSet<u64>>,
    /// The selected row of the accessibility panel: a violation or a node.
    pub(crate) access_selected: RwSignal<Option<Arc<str>>>,
    pub(crate) access_expanded: RwSignal<HashSet<Arc<str>>>,
    pub(crate) outline: RwSignal<bool>,
    pub(crate) measure: RwSignal<bool>,
    pub(crate) details_height: RwSignal<f32>,
    /// The details' width while they stand beside the tree, kept apart from their height below it so turning the panel keeps each.
    pub(crate) details_width: RwSignal<f32>,
    /// How many inspector panels are mounted: the selection is marked only while one is.
    pub(crate) inspecting: RwSignal<u32>,
    /// How many accessibility panels are mounted: the audit is read only while one is.
    pub(crate) auditing: RwSignal<u32>,
    read: RwSignal<Option<u64>>,
}

pub(crate) const DETAILS_HEIGHT: f32 = 220.0;
pub(crate) const DETAILS_WIDTH: f32 = 280.0;

impl Inspection {
    pub fn new() -> Self {
        Self {
            nodes: signal(Rc::from([])),
            audit: signal(None),
            placement: signal(Transform::IDENTITY),
            selected: signal(None),
            expanded: signal(HashSet::new()),
            seen_branches: signal(HashSet::new()),
            access_selected: signal(None),
            access_expanded: signal(HashSet::from([
                Arc::from(crate::panels::VIOLATIONS_ROW),
                Arc::from(crate::panels::READER_ROW),
            ])),
            outline: signal(false),
            measure: signal(false),
            details_height: signal(DETAILS_HEIGHT),
            details_width: signal(DETAILS_WIDTH),
            inspecting: signal(0),
            auditing: signal(0),
            read: signal(None),
        }
    }

    /// Whether an accessibility panel is mounted and so wants [`InspectSource::audit`] read.
    pub fn wants_audit(&self) -> bool {
        self.auditing.peek() > 0
    }

    /// The components the last refresh read, in pre-order.
    pub fn nodes(&self) -> Rc<[InspectedNode]> {
        self.nodes.get()
    }

    /// What the last refresh read of what a reader is told; `None` where the source could not say or no accessibility panel asked.
    pub fn audit(&self) -> Option<Rc<AccessAudit>> {
        self.audit.get()
    }

    /// Where the marks for the source's rects are drawn, as the last refresh read it: see [`InspectSource::placement`].
    pub fn placement(&self) -> Transform {
        self.placement.get()
    }

    /// Shows or hides every component's box over the source.
    pub fn outline(&self) -> RwSignal<bool> {
        self.outline
    }

    /// Shows or hides the selected component's size and its distance to the box around it.
    pub fn measure(&self) -> RwSignal<bool> {
        self.measure
    }

    /// Selects the component the walk numbers `id`, or none.
    pub fn select(&self, id: Option<u64>) {
        self.selected
            .set(id.map(|id| Arc::from(id.to_string().as_str())));
    }

    /// The selected component, as the last walk reported it.
    pub fn selected_node(&self) -> Option<InspectedNode> {
        let id: u64 = self.selected.get()?.parse().ok()?;
        self.nodes
            .with(|nodes| nodes.iter().find(|node| node.id == id).cloned())
    }

    /// Reads `source` again where it drew something new since the last read, and its placement every time; only what changed reaches the panels.
    pub fn refresh(&self, source: &dyn InspectSource) {
        telar::batch(|| {
            self.placement.set_if_changed(source.placement());
            let generation = source.generation();
            let audit_wanted = self.wants_audit();
            let stale = self.read.peek() != Some(generation);
            if stale {
                self.read.set(Some(generation));
                self.take_walk(&source.walk());
            }
            // An audit nobody shows goes stale unread, so it is dropped and read afresh when a panel asks again.
            if !audit_wanted {
                if self.audit.peek_with(Option::is_some) {
                    self.audit.set(None);
                }
            } else if stale || self.audit.peek_with(Option::is_none) {
                let audit = source.audit();
                let same = self
                    .audit
                    .peek_with(|current| current.as_deref() == audit.as_ref());
                if !same {
                    self.audit.set(audit.map(Rc::new));
                }
            }
        });
    }

    /// Keeps reading `source` after every layout pass of the tree this is called in, for as long as the returned hook lives: what a workshop calls once it mounts a canvas.
    ///
    /// A new source starts with nothing chosen in it, and is read whole on the first pass whatever the last one's generation was.
    pub fn follow(self, source: impl InspectSource + 'static) -> RelayoutHook {
        telar::batch(|| {
            self.read.set(None);
            self.selected.set(None);
            self.access_selected.set(None);
        });
        on_relayout(move || self.refresh(&source))
    }

    fn take_walk(&self, walk: &[SegmentNodeInfo]) {
        self.keep_a_selection(walk);
        let same = self.nodes.peek_with(|current| {
            current.len() == walk.len()
                && current
                    .iter()
                    .zip(walk)
                    .all(|(kept, read)| kept.reads_as(read))
        });
        if same {
            return;
        }
        let opened: Vec<Arc<str>> = self.seen_branches.peek_with(|seen| {
            branches(walk)
                .filter(|id| !seen.contains(id))
                .map(|id| Arc::from(id.to_string()))
                .collect()
        });
        if !opened.is_empty() {
            self.seen_branches
                .update(|seen| seen.extend(branches(walk)));
            self.expanded.update(|open| open.extend(opened));
        }
        self.nodes
            .set(walk.iter().map(InspectedNode::of).collect::<Rc<[_]>>());
    }

    /// Selects the outermost component while nothing is, or what was is gone, so the details always show one where there is any.
    fn keep_a_selection(&self, walk: &[SegmentNodeInfo]) {
        let kept = self.selected.peek_with(|selected| {
            selected
                .as_deref()
                .and_then(|id| id.parse::<u64>().ok())
                .is_some_and(|id| walk.iter().any(|node| node.id == id))
        });
        if !kept {
            self.selected
                .set(walk.first().map(|root| Arc::from(root.id.to_string())));
        }
    }
}

impl Default for Inspection {
    fn default() -> Self {
        Self::new()
    }
}

/// The ids of the nodes in a pre-order walk that have nodes under them.
fn branches(nodes: &[SegmentNodeInfo]) -> impl Iterator<Item = u64> + '_ {
    nodes
        .windows(2)
        .filter(|pair| pair[1].depth > pair[0].depth)
        .map(|pair| pair[0].id)
}

#[cfg(test)]
#[path = "inspect_test.rs"]
mod tests;
