//! The accessibility panel: what the checks found, and what a reader is told, in reading order.

use telar::{
    AccessAudit, AccessNode, AccessSeverity, AlignItems, Children, Container, LayoutError,
    LayoutItem, LayoutStyle, Reactive, Text, box_item, children, memo,
};
use telar_components::{TreeNode, TreeViewProps, tree_view};

use super::count_mounted;
use crate::inspect::Inspection;
use crate::strings::{
    self, ACCESSIBILITY, AUDIT_CLEAN, AUDIT_UNAVAILABLE, ERRORS, FOCUSED, PROBLEMS, READER,
    UNNAMED, WARNINGS,
};
use crate::workbench::{WORKBENCH_GRID, workbench_fill, workbench_muted};

pub(crate) const VIOLATIONS_ROW: &str = "violations";
pub(crate) const READER_ROW: &str = "reader";

/// What the accessibility checks found in what `inspection` reads, over everything a reader is told about it, in reading order. Choosing a row marks where it is.
///
/// While one is mounted the inspection reads the audit; with none, nothing pays for it.
pub fn accessibility_panel(inspection: Inspection) -> Result<Box<dyn LayoutItem>, LayoutError> {
    count_mounted(inspection.auditing);
    let summary = Text::declaring(
        move || match inspection.audit.get() {
            None => strings::text(AUDIT_UNAVAILABLE),
            Some(audit) => summary(&audit),
        },
        LayoutStyle::new(),
        workbench_muted,
    )?;
    let header = Container::new(
        LayoutStyle::row()
            .flex_shrink(0.0)
            .align_items(AlignItems::CENTER)
            .padding_horizontal(WORKBENCH_GRID * 1.5)
            .padding_vertical(WORKBENCH_GRID / 2.0),
        children![summary],
    )?;
    let items = memo(move || {
        inspection
            .audit
            .get()
            .map(|audit| audit_rows(&audit))
            .unwrap_or_default()
    });
    let tree = tree_view(
        TreeViewProps::props()
            .items(items)
            .selected(inspection.access_selected)
            .expanded(inspection.access_expanded)
            .label(Reactive::of(|| strings::text(ACCESSIBILITY)))
            .build(),
        Children::default(),
    )?;
    Ok(box_item(Container::new(
        workbench_fill(),
        vec![box_item(header), tree],
    )?))
}

fn summary(audit: &AccessAudit) -> String {
    let count = |severity| {
        audit
            .violations
            .iter()
            .filter(|violation| violation.severity == severity)
            .count()
    };
    let (errors, warnings) = (count(AccessSeverity::Error), count(AccessSeverity::Warning));
    if errors + warnings == 0 {
        return strings::text(AUDIT_CLEAN);
    }
    format!(
        "{} · {}",
        strings::text_with(ERRORS, &errors.to_string()),
        strings::text_with(WARNINGS, &warnings.to_string())
    )
}

/// The panel's rows: the violations under one branch and the reader's nodes under another, each named by its place, which [`chosen_rect`] reads back.
pub(crate) fn audit_rows(audit: &AccessAudit) -> Vec<TreeNode> {
    let violations = audit
        .violations
        .iter()
        .enumerate()
        .map(|(index, violation)| {
            TreeNode::new(violation_row(index), violation.message.clone()).with_badge(format!(
                "{} · {}",
                violation.severity.as_str(),
                violation.rule.id()
            ))
        });
    let nodes = audit.nodes.iter().enumerate().map(|(index, node)| {
        TreeNode::new(node_row(index), name_of(node)).with_badge(badge_of(node))
    });
    vec![
        TreeNode::new(VIOLATIONS_ROW, strings::text(PROBLEMS))
            .with_badge(audit.violations.len().to_string())
            .with_children(violations),
        TreeNode::new(READER_ROW, strings::text(READER))
            .with_badge(audit.nodes.len().to_string())
            .with_children(nodes),
    ]
}

fn violation_row(index: usize) -> String {
    format!("{VIOLATIONS_ROW}/{index}")
}

fn node_row(index: usize) -> String {
    format!("{READER_ROW}/{index}")
}

fn name_of(node: &AccessNode) -> String {
    match node.name.trim() {
        "" => strings::text(UNNAMED),
        name => name.to_owned(),
    }
}

fn badge_of(node: &AccessNode) -> String {
    let mut badge = node.role.as_str().to_owned();
    if node.focused {
        badge.push_str(" · ");
        badge.push_str(&strings::text(FOCUSED));
    }
    badge
}

/// Where the row `chosen` stands for, in the audit's coordinates: the violation's rect, or the node's.
pub(crate) fn chosen_rect(audit: &AccessAudit, chosen: &str) -> Option<telar::Rect> {
    let (branch, index) = chosen.split_once('/')?;
    let index: usize = index.parse().ok()?;
    match branch {
        VIOLATIONS_ROW => audit.violations.get(index)?.rect,
        READER_ROW => audit.nodes.get(index).map(|node| node.rect),
        _ => None,
    }
}
