use std::collections::BTreeSet;
use std::sync::Arc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Color, Container, Key, LayoutError, LayoutItem, LayoutStyle, NamedKey, Role, signal};

use crate::data_table::{CellAlign, Column, DataTableProps, TableRow, data_table};
use crate::selection::SelectionMode;

fn columns() -> Vec<Column> {
    vec![
        Column::new("name", "Name")
            .flex(2.0)
            .sortable(true)
            .resizable(true)
            .min_width(120.0),
        Column::new("kind", "Kind").sortable(true),
        Column::new("size", "Size")
            .fixed(88.0)
            .align(CellAlign::End)
            .sortable(true)
            .resizable(true),
    ]
}

fn files() -> Vec<TableRow> {
    const KINDS: [(&str, &str); 5] = [
        ("rs", "Rust"),
        ("toml", "Manifest"),
        ("md", "Markdown"),
        ("png", "Image"),
        ("rsx", "View"),
    ];
    (0..500)
        .map(|i| {
            let (extension, kind) = KINDS[i % KINDS.len()];
            TableRow::new(
                format!("file-{i}"),
                [
                    format!("file_{:03}.{extension}", (i * 37) % 500),
                    kind.to_string(),
                    format!("{}", (i * 7919) % 9000 + 12),
                ],
            )
        })
        .collect()
}

fn task_columns() -> Vec<Column> {
    vec![
        Column::new("task", "Task")
            .flex(3.0)
            .wrap(true)
            .sortable(true),
        Column::new("owner", "Owner").sortable(true),
        Column::new("due", "Due").fixed(64.0).align(CellAlign::End),
    ]
}

fn tasks() -> Vec<TableRow> {
    const TASKS: [(&str, &str, &str); 6] = [
        ("Ship the release notes", "Ada", "Mon"),
        (
            "Rewrite the onboarding guide so each step says what it is for before what to type",
            "Grace",
            "Tue",
        ),
        ("Fix the flaky test", "Alan", "Wed"),
        (
            "Check keyboard paging in the tree, the table and the list against rows of different heights",
            "Barbara",
            "Thu",
        ),
        ("Order more coffee", "Ken", "Fri"),
        (
            "Move the nightly build to the new runners, keeping the old ones for a week",
            "Edsger",
            "Mon",
        ),
    ];
    (0..60)
        .map(|i| {
            let (task, owner, due) = TASKS[i % TASKS.len()];
            TableRow::new(format!("task-{i}"), [task, owner, due])
        })
        .collect()
}

fn framed(table: Box<dyn LayoutItem>) -> Result<Container, LayoutError> {
    Container::new(
        LayoutStyle::column().width(420.0).height(320.0),
        vec![table],
    )
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(data_table: DataTableProps, "Files", |p| {
            let table = data_table(
                DataTableProps::props()
                    .columns(columns())
                    .rows(files())
                    .label(p.arg("label", "Files"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
            )?;
            Ok::<_, LayoutError>(framed(table)?)
        })
        .title("Display/Data table")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect(by_role(Role::ColumnHeader).named("Name"))?;
            canvas.click(by_role(Role::ColumnHeader).named("Name"))?;
            canvas.expect_action("on_sort", 1)?;
            canvas.expect(by_role(Role::GridCell).named("file_000.rs"))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_action("on_select", 1)?;
            canvas.expect_value(by_role(Role::Splitter).named("Resize Size column"), 88.0)?;
            canvas.drag(
                by_role(Role::Splitter).named("Resize Size column"),
                24.0,
                0.0,
            )?;
            canvas.expect_value(by_role(Role::Splitter).named("Resize Size column"), 112.0)?;
            canvas.tab_back()?;
            canvas.expect_focused(by_role(Role::Splitter).named("Resize Name column"))?;
            canvas.press(Key::Named(NamedKey::Home))?;
            canvas.expect_value(by_role(Role::Splitter).named("Resize Name column"), 120.0)?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            canvas.expect_value(by_role(Role::Splitter).named("Resize Name column"), 136.0)?;
            canvas.expect_action("on_sort", 1)
        }),
        preview!(data_table: DataTableProps, "Several rows at once", |p| {
            let chosen = BTreeSet::from([Arc::from("file-1")]);
            let table = data_table(
                DataTableProps::props()
                    .columns(columns())
                    .rows(files())
                    .selection_mode(p.arg("selection_mode", SelectionMode::Multiple))
                    .selected_rows(signal(chosen))
                    .label(p.arg("label", "Files"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
            )?;
            Ok::<_, LayoutError>(framed(table)?)
        })
        .title("Display/Data table")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::GridCell).named("file_074.md"))?;
            canvas.expect_action("on_select", 1)?;
            canvas.expect_action("on_selection_change", 1)?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Space))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Space))?;
            canvas.expect_action("on_selection_change", 3)?;
            canvas.press(Key::Named(NamedKey::Space))?;
            canvas.expect_action("on_selection_change", 4)?;
            canvas.expect_action("on_select", 1)
        }),
        preview!(data_table: DataTableProps, "Wrapped cells", |p| {
            let table = data_table(
                DataTableProps::props()
                    .columns(task_columns())
                    .rows(tasks())
                    .label(p.arg("label", "Tasks"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
            )?;
            Ok::<_, LayoutError>(framed(table)?)
        })
        .title("Display/Data table")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect(
                by_role(Role::GridCell)
                    .named("Ship the release notes")
                    .nth(0),
            )?;
            canvas.click(by_role(Role::GridCell).named("Fix the flaky test").nth(0))?;
            canvas.expect_action("on_select", 1)?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_action("on_select", 2)
        }),
        preview!(data_table: DataTableProps, "Empty", |p| {
            let table = data_table(
                DataTableProps::props()
                    .columns(columns())
                    .label(p.arg("label", "Files"))
                    .empty_label(p.arg("empty_label", "No files match"))
                    .build(),
            )?;
            Ok::<_, LayoutError>(framed(table)?)
        })
        .title("Display/Data table")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect(by_role(Role::ColumnHeader).named("Name"))?;
            canvas.expect_text("No files match")?;
            canvas.expect_absent(by_role(Role::GridCell))
        }),
    ]
}
