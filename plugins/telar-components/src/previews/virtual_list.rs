use std::sync::Arc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Container, Key, LayoutError, LayoutStyle, NamedKey, Role, signal};

use crate::virtual_list::{ListRow, VirtualListProps, virtual_list};

fn contacts() -> Vec<ListRow> {
    const FIRST: [&str; 8] = [
        "Ada", "Alan", "Grace", "Edsger", "Barbara", "Donald", "Frances", "Ken",
    ];
    const LAST: [&str; 6] = [
        "Lovelace", "Turing", "Hopper", "Dijkstra", "Liskov", "Knuth",
    ];
    (0..2_000)
        .map(|i| {
            let name = format!(
                "{} {}",
                FIRST[i % FIRST.len()],
                LAST[(i / FIRST.len()) % LAST.len()]
            );
            ListRow::new(format!("contact-{i}"), name)
                .with_detail(format!("#{i}"))
                .disabled(i % 13 == 5)
        })
        .collect()
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(virtual_list: VirtualListProps, "Two thousand rows", |p| {
            let list = virtual_list(
                VirtualListProps::props()
                    .items(contacts())
                    .selected(signal(Some(Arc::from("contact-2"))))
                    .label(p.arg("label", "Contacts"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().width(280.0).height(320.0),
                vec![list],
            )?)
        })
        .title("Layout/Virtual list")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect_toggled(by_role(Role::Option).named("Grace Lovelace #2"), true)?;
            canvas.click(by_role(Role::Option).named("Edsger Lovelace #3"))?;
            canvas.expect_toggled(by_role(Role::Option).named("Edsger Lovelace #3"), true)?;
            canvas.expect_action("on_select", 1)?;
            canvas.press(Key::Named(NamedKey::End))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_action("on_select", 2)?;
            canvas.expect_toggled(by_role(Role::Option).named("Ken Dijkstra #1999"), true)
        }),
    ]
}
