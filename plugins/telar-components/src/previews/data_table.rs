use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Container, Key, LayoutError, LayoutStyle, NamedKey, Role};

use crate::data_table::{CellAlign, Column, DataTableProps, TableRow, data_table};

fn columns() -> Vec<Column> {
    vec![
        Column::new("name", "Name").flex(2.0).sortable(true),
        Column::new("kind", "Kind").sortable(true),
        Column::new("size", "Size")
            .fixed(88.0)
            .align(CellAlign::End)
            .sortable(true),
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
                Children::default(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().width(420.0).height(320.0),
                vec![table],
            )?)
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
            canvas.expect_action("on_select", 1)
        }),
    ]
}
