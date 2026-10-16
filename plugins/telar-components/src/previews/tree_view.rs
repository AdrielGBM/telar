use std::collections::HashSet;
use std::sync::Arc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Container, Key, LayoutError, LayoutStyle, NamedKey, Role, signal};

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

fn chapters() -> Vec<TreeNode> {
    vec![
        TreeNode::new("start", "Getting started").with_children([
            TreeNode::new(
                "start/install",
                "Installing the command-line tools and the editor support",
            ),
            TreeNode::new(
                "start/first",
                "Your first window, from an empty project to a running app",
            )
            .with_badge("new"),
        ]),
        TreeNode::new("views", "Views").with_children([
            TreeNode::new("views/rsx", "Writing a view in .rsx"),
            TreeNode::new(
                "views/lists",
                "Long lists, and why a virtual one builds only the rows that are on screen",
            ),
        ]),
        TreeNode::new(
            "themes",
            "Themes, tokens and the control size every component scales with",
        ),
        TreeNode::new("faq", "FAQ"),
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
                LayoutStyle::column().width(260.0).height(320.0),
                vec![tree],
            )?)
        })
        .title("Workbench/Tree view")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let tests = by_role(Role::TreeItem).named("tests");
            canvas.click(by_role(Role::TreeItem).named("main.rs"))?;
            canvas.expect_toggled(by_role(Role::TreeItem).named("main.rs"), true)?;
            canvas.expect_toggled(by_role(Role::TreeItem).named("lib.rs M"), false)?;
            canvas.expect_action("on_select", 1)?;
            canvas.expect_absent(by_role(Role::TreeItem).named("smoke.rs"))?;
            canvas.click(tests.clone())?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            canvas.expect_expanded(tests, true)?;
            canvas.expect(by_role(Role::TreeItem).named("smoke.rs"))
        }),
        preview!(tree_view: TreeViewProps, "Wrapped labels", |p| {
            let expanded: HashSet<Arc<str>> = ["start", "views"].into_iter().map(Arc::from).collect();
            let tree = tree_view(
                TreeViewProps::props()
                    .items(chapters())
                    .wrap(p.arg("wrap", true))
                    .selected(signal(Some(Arc::from("views/lists"))))
                    .expanded(signal(expanded))
                    .label(p.arg("label", "Guide"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().width(260.0).height(320.0),
                vec![tree],
            )?)
        })
        .title("Workbench/Tree view")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let rsx = by_role(Role::TreeItem).named("Writing a view in .rsx");
            canvas.click(rsx.clone())?;
            canvas.expect_toggled(rsx, true)?;
            canvas.expect_action("on_select", 1)?;
            canvas.press(Key::Named(NamedKey::Home))?;
            canvas.press(Key::Named(NamedKey::ArrowLeft))?;
            canvas.expect_expanded(by_role(Role::TreeItem).named("Getting started"), false)
        }),
    ]
}
