//! What the sidebar lists: the previews grouped by their title path into a tree, narrowed by a query.

use std::sync::Arc;

use telar::preview::PreviewEntry;
use telar_components::TreeNode;

/// The tree of `entries` whose title, name or tags match `query`, grouped by title path, with `badge` naming what a preview's row carries. An empty query keeps every preview.
pub(super) fn tree(
    entries: &[PreviewEntry],
    query: &str,
    badge: impl Fn(&PreviewEntry) -> Option<String>,
) -> Vec<TreeNode> {
    let mut root = Vec::new();
    for entry in entries.iter().filter(|entry| matches(query, entry)) {
        insert(&mut root, "", &segments(entry), *entry);
    }
    root.iter().map(|group| group.node(&badge)).collect()
}

/// The id of every group `entries` are listed under.
pub(crate) fn group_ids(entries: &[PreviewEntry]) -> Vec<Arc<str>> {
    let mut ids = Vec::new();
    collect_branches(&tree(entries, "", |_| None), &mut ids);
    ids
}

/// The id of every branch of `nodes`, at any depth.
pub(super) fn collect_branches(nodes: &[TreeNode], ids: &mut Vec<Arc<str>>) {
    for node in nodes.iter().filter(|node| node.is_branch()) {
        ids.push(node.id.clone());
        collect_branches(&node.children, ids);
    }
}

fn matches(query: &str, entry: &PreviewEntry) -> bool {
    query.split_whitespace().all(|token| {
        [entry.title, entry.name]
            .into_iter()
            .chain(entry.tags.iter().copied())
            .any(|field| is_subsequence(token, field))
    })
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut haystack = haystack.chars().flat_map(char::to_lowercase);
    needle
        .chars()
        .flat_map(char::to_lowercase)
        .all(|wanted| haystack.any(|found| found == wanted))
}

fn segments(entry: &PreviewEntry) -> Vec<&'static str> {
    let path: Vec<_> = entry
        .title
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    if path.is_empty() {
        vec![entry.component]
    } else {
        path
    }
}

/// A group of the title path. Its id starts with `/`, which no preview id does, so a group and a preview never share one.
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
