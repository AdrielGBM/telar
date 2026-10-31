//! What the sidebar lists: the previews grouped by their title path into a tree, under their package when they come from more than one, narrowed by a query.

use std::sync::Arc;

use telar::preview::PreviewEntry;
use telar::preview::host::title_path;
use telar_components::{TreeNode, fuzzy_match};

/// The tree of `entries` whose package, title, name or tags match `query`, grouped by package when there is more than one and then by title path, with `badge` naming what a preview's row carries. An empty query keeps every preview.
pub(super) fn tree(
    entries: &[PreviewEntry],
    query: &str,
    badge: impl Fn(&PreviewEntry) -> Option<String>,
) -> Vec<TreeNode> {
    let by_package = spans_packages(entries);
    let mut root = Vec::new();
    for entry in entries.iter().filter(|entry| matches(query, entry)) {
        insert(&mut root, "", &segments(entry, by_package), *entry);
    }
    root.iter().map(|group| group.node(&badge)).collect()
}

/// The previews `query` keeps, in the order the sidebar lists them: by package and title path, each group's subgroups before its own previews.
pub(crate) fn listed(entries: &[PreviewEntry], query: &str) -> Vec<PreviewEntry> {
    let mut ids = Vec::new();
    collect_leaves(&tree(entries, query, |_| None), &mut ids);
    ids.iter()
        .filter_map(|id| entries.iter().find(|entry| *entry.id == **id).copied())
        .collect()
}

fn collect_leaves(nodes: &[TreeNode], ids: &mut Vec<Arc<str>>) {
    for node in nodes {
        if node.is_branch() {
            collect_leaves(&node.children, ids);
        } else {
            ids.push(node.id.clone());
        }
    }
}

/// The id of every group `entries` are listed under.
pub(crate) fn group_ids(entries: &[PreviewEntry]) -> Vec<Arc<str>> {
    TreeNode::branch_ids(&tree(entries, "", |_| None))
}

/// The id of every group the preview `id` is listed under, outermost first; empty for an id no preview has.
pub(crate) fn groups_holding(entries: &[PreviewEntry], id: &str) -> Vec<Arc<str>> {
    let mut path = Vec::new();
    path_to(&tree(entries, "", |_| None), id, &mut path);
    path
}

fn path_to(nodes: &[TreeNode], id: &str, path: &mut Vec<Arc<str>>) -> bool {
    nodes.iter().any(|node| {
        if !node.is_branch() {
            return *node.id == *id;
        }
        path.push(node.id.clone());
        let found = path_to(&node.children, id, path);
        if !found {
            path.pop();
        }
        found
    })
}

fn matches(query: &str, entry: &PreviewEntry) -> bool {
    query.split_whitespace().all(|token| {
        [entry.package, entry.title, entry.name]
            .into_iter()
            .chain(entry.tags.iter().copied())
            .any(|field| fuzzy_match(token, field).is_some())
    })
}

/// Whether `entries` come from more than one package. Read over every preview rather than the ones a search keeps, so the tree keeps its shape as a search narrows it.
fn spans_packages(entries: &[PreviewEntry]) -> bool {
    entries
        .split_first()
        .is_some_and(|(first, rest)| rest.iter().any(|entry| entry.package != first.package))
}

fn segments(entry: &PreviewEntry, by_package: bool) -> Vec<&'static str> {
    let mut path: Vec<_> = title_path(entry.title).collect();
    if path.is_empty() {
        path.push(entry.component);
    }
    if by_package && !entry.package.is_empty() {
        path.insert(0, entry.package);
    }
    path
}

/// A package, or a group of the title path. Its id starts with `/`, which no preview id does, so a group and a preview never share one.
struct Group {
    label: &'static str,
    path: String,
    groups: Vec<Group>,
    previews: Vec<PreviewEntry>,
}

impl Group {
    fn count(&self) -> usize {
        self.previews.len() + self.groups.iter().map(Group::count).sum::<usize>()
    }

    fn node(&self, badge: &impl Fn(&PreviewEntry) -> Option<String>) -> TreeNode {
        let groups = self.groups.iter().map(|group| group.node(badge));
        let previews = self.previews.iter().map(|entry| {
            let leaf = TreeNode::new(entry.id, entry.name);
            match badge(entry) {
                Some(badge) => leaf.with_badge(badge),
                None => leaf,
            }
        });
        TreeNode::new(self.path.as_str(), self.label)
            .with_badge(self.count().to_string())
            .with_children(groups.chain(previews))
    }
}

fn insert(groups: &mut Vec<Group>, parent: &str, path: &[&'static str], entry: PreviewEntry) {
    let Some((&label, rest)) = path.split_first() else {
        return;
    };
    let at = match groups.iter().position(|group| group.label == label) {
        Some(at) => at,
        None => {
            groups.push(Group {
                label,
                path: format!("{parent}/{label}"),
                groups: Vec::new(),
                previews: Vec::new(),
            });
            groups.len() - 1
        }
    };
    let group = &mut groups[at];
    if rest.is_empty() {
        group.previews.push(entry);
    } else {
        let parent = group.path.clone();
        insert(&mut group.groups, &parent, rest, entry);
    }
}
