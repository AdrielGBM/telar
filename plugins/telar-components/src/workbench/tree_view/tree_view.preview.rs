use std::collections::HashSet;
use std::rc::Rc;

use super::sample::{Given, expect_names};
use telar::preview::{Layout, Matrix, Play, PlayResult, PreviewEntry, Query, by_role, preview};
use telar::{
    Color, Container, DragSession, Key, LayoutError, LayoutStyle, NamedKey, Role, RwSignal,
    signal,
};

use crate::workbench::icon_button::icon_button::picture;
use crate::workbench::tree_view::tree_drag::{TreeDrag, TreeTarget};
use crate::workbench::tree_view::tree_view::{TreeNode, TreeViewProps, tree_view};
use telar::ControlSize;

fn project() -> Vec<TreeNode<&'static str>> {
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

fn chapters() -> Vec<TreeNode<&'static str>> {
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

/// What a reading library names its shelves by: a folder or a text, each by its title.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Shelf {
    Folder(&'static str),
    Text(&'static str),
}

fn library() -> Vec<TreeNode<Shelf>> {
    let text = |title| TreeNode::new(Shelf::Text(title), title);
    vec![
        TreeNode::new(Shelf::Folder("Reading"), "Reading")
            .with_children([text("Kokoro"), text("Rashomon")]),
        TreeNode::new(Shelf::Folder("Archive"), "Archive").with_children([]),
        text("Notes"),
    ]
}

/// `nodes` with the node `id` taken out of wherever it is, and that node.
fn take_out(
    nodes: &[TreeNode<Shelf>],
    id: Shelf,
) -> (Vec<TreeNode<Shelf>>, Option<TreeNode<Shelf>>) {
    let mut taken = None;
    let kept = nodes
        .iter()
        .filter_map(|node| {
            if node.id == id {
                taken = Some(node.clone());
                return None;
            }
            let mut node = node.clone();
            if let Some(children) = &node.children {
                let (children, found) = take_out(children, id);
                taken = taken.take().or(found);
                node.children = Some(children.into());
            }
            Some(node)
        })
        .collect();
    (kept, taken)
}

/// `nodes` with `moving` put where `target` says, among the nodes under `under` (`None` for the top level).
fn put(
    nodes: Vec<TreeNode<Shelf>>,
    under: Option<Shelf>,
    moving: &TreeNode<Shelf>,
    target: &TreeTarget<Shelf>,
) -> Vec<TreeNode<Shelf>> {
    let lands_here = match target {
        TreeTarget::Inside(into) => under == Some(*into),
        TreeTarget::Between { parent, .. } => under == *parent,
    };
    let mut nodes: Vec<TreeNode<Shelf>> = nodes
        .into_iter()
        .map(|mut node| {
            if let Some(children) = &node.children {
                node.children = Some(put(children.to_vec(), Some(node.id), moving, target).into());
            }
            node
        })
        .collect();
    if lands_here {
        let at = match target {
            TreeTarget::Between {
                before: Some(before),
                ..
            } => nodes.iter().position(|node| node.id == *before),
            _ => None,
        };
        nodes.insert(at.unwrap_or(nodes.len()), moving.clone());
    }
    nodes
}

/// Moves the node `id` of `shelves` to `target`.
fn shelve(shelves: RwSignal<Vec<TreeNode<Shelf>>>, id: Shelf, target: &TreeTarget<Shelf>) {
    shelves.update(|nodes| {
        let (kept, taken) = take_out(nodes, id);
        if let Some(moving) = taken {
            *nodes = put(kept, None, &moving, target);
        }
    });
}

/// Carries the row `from` finds onto the row `to` finds, `down` of the way down it.
fn drag_row(canvas: &mut Play, from: Query, to: Query, down: f32) -> PlayResult {
    let start = canvas.find(&from)?.rect;
    let end = canvas.find(&to)?.rect;
    let dy = end.y + end.height * down - (start.y + start.height / 2.0);
    canvas.drag(from, 0.0, dy)
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(tree_view: TreeViewProps<&'static str>, "Project", |p| {
            let expanded: HashSet<&str> =
                ["src", "src/ui"].into_iter().collect();
            let tree = tree_view(
                TreeViewProps::props()
                    .on_select(|_| {})
                    .items(project())
                    .selected(signal(Some("src/lib.rs")))
                    .expanded(signal(expanded))
                    .select_branches(p.arg("select_branches", true))
                    .label(p.arg("label", "Files"))
                    .size(p.arg("size", ControlSize::Regular))
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .build(),
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
        preview!(tree_view: TreeViewProps<&'static str>, "Wrapped labels", |p| {
            let expanded: HashSet<&str> =
                ["start", "views"].into_iter().collect();
            let tree = tree_view(
                TreeViewProps::props()
                    .on_select(|_| {})
                    .items(chapters())
                    .wrap(p.arg("wrap", true))
                    .selected(signal(Some("views/lists")))
                    .expanded(signal(expanded))
                    .label(p.arg("label", "Guide"))
                    .given(p.arg("color", None::<Color>), |props, color| props.color(color))
                    .build(),
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
        preview!(tree_view: TreeViewProps<Shelf>, "Library", |p| {
            let shelves = signal(library());
            let session = DragSession::<Shelf>::new();
            let drag = TreeDrag::new(
                &session,
                |node: &TreeNode<Shelf>| Some(node.id),
                |carried: &Shelf, target: &TreeTarget<Shelf>| {
                    matches!(
                        (carried, target),
                        (_, TreeTarget::Between { .. })
                            | (Shelf::Text(_), TreeTarget::Inside(Shelf::Folder(_)))
                    )
                },
                move |carried: Shelf, target| shelve(shelves, carried, &target),
            );
            let size = p.arg("size", ControlSize::Regular);
            let tree = tree_view(
                TreeViewProps::props()
                    .items(telar::Reactive::of(move || shelves.get()))
                    .expanded(signal(HashSet::from([Shelf::Folder("Reading")])))
                    .drag(drag)
                    .leading(Rc::new(move |node: &TreeNode<Shelf>| {
                        let icon = match node.id {
                            Shelf::Folder(_) => telar::icon!("lucide:folder"),
                            Shelf::Text(_) => telar::icon!("lucide:file-text"),
                        };
                        picture(icon, size, || None)
                    }))
                    .on_key(Rc::new(move |key: &Key, node: Option<&TreeNode<Shelf>>| {
                        let (Key::Named(NamedKey::Delete), Some(node)) = (key, node) else {
                            return false;
                        };
                        let id = node.id;
                        shelves.update(|nodes| *nodes = take_out(nodes, id).0);
                        true
                    }))
                    .size(size)
                    .label(p.arg("label", "Library"))
                    .build(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().width(260.0).height(320.0),
                vec![tree],
            )?)
        })
        .title("Workbench/Tree view")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            let row = |name| by_role(Role::TreeItem).named(name);
            drag_row(canvas, row("Notes"), row("Archive"), 0.5)?;
            drag_row(canvas, row("Rashomon"), row("Kokoro"), 0.1)?;
            canvas.click(row("Kokoro"))?;
            canvas.press(Key::Named(NamedKey::Delete))?;
            canvas.click(row("Archive"))?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            expect_names(
                canvas,
                &by_role(Role::TreeItem),
                &["Reading", "Rashomon", "Archive", "Notes"],
            )
        }),
    ]
}
