use std::collections::HashSet;
use std::sync::Arc;

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color, Container, LayoutError, LayoutStyle, signal};

use crate::tree_view::{TreeNode, TreeViewProps, tree_view};

fn project() -> Vec<TreeNode> {
    vec![
        TreeNode::new("src", "src").with_children([
            TreeNode::new("src/main.rs", "main.rs"),
            TreeNode::new("src/lib.rs", "lib.rs").with_badge("M"),
            TreeNode::new("src/ui", "ui").with_children([
                TreeNode::new("src/ui/button.rs", "button.rs"),
                TreeNode::new("src/ui/theme.rs", "theme.rs").with_badge("A"),
            ]),
        ]),
        TreeNode::new("tests", "tests")
            .with_children([TreeNode::new("tests/smoke.rs", "smoke.rs")]),
        TreeNode::new("Cargo.toml", "Cargo.toml"),
        TreeNode::new("README.md", "README.md"),
    ]
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(tree_view: TreeViewProps, "Project", |p| {
            let expanded: HashSet<Arc<str>> = ["src", "src/ui"].into_iter().map(Arc::from).collect();
            let tree = tree_view(
                TreeViewProps::props()
                    .items(project())
                    .selected(signal(Some(Arc::from("src/lib.rs"))))
                    .expanded(signal(expanded))
                    .select_branches(p.arg("select_branches", true))
                    .label(p.arg("label", "Files"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column().width(260.0).height(320.0),
                vec![tree],
            )?)
        })
        .title("Workbench/Tree view")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
