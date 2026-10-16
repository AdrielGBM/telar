use std::collections::BTreeSet;
use std::sync::Arc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Color, Container, Key, LayoutError, LayoutStyle, NamedKey, Role, signal};

use crate::selection::SelectionMode;
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

fn messages() -> Vec<ListRow> {
    const SAID: [&str; 5] = [
        "Lunch?",
        "The build is green again: the flaky test was waiting on a clock it never started",
        "Can you look at the table before Friday?",
        "Notes from the review: rows of different heights page by what the view shows whole, and the cursor's row is always brought in whole",
        "Thanks!",
    ];
    const FROM: [&str; 4] = ["Ada", "Grace", "Alan", "Ken"];
    (0..500)
        .map(|i| {
            let said = SAID[i % SAID.len()];
            ListRow::new(
                format!("message-{i}"),
                format!("{}: {said}", FROM[i % FROM.len()]),
            )
            .with_detail(format!("{}:{:02}", 9 + i / 60 % 9, i % 60))
        })
        .collect()
}

fn framed(list: Box<dyn telar::LayoutItem>) -> Result<Container, LayoutError> {
    Container::new(LayoutStyle::column().width(280.0).height(320.0), vec![list])
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
            Ok::<_, LayoutError>(framed(list)?)
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
        preview!(virtual_list: VirtualListProps, "Several at once", |p| {
            let chosen = BTreeSet::from([Arc::from("contact-2"), Arc::from("contact-6")]);
            let list = virtual_list(
                VirtualListProps::props()
                    .items(contacts())
                    .selection_mode(p.arg("selection_mode", SelectionMode::Multiple))
                    .selected_rows(signal(chosen))
                    .label(p.arg("label", "Recipients"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(framed(list)?)
        })
        .title("Layout/Virtual list")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect_toggled(by_role(Role::Option).named("Grace Lovelace #2"), true)?;
            canvas.expect_toggled(by_role(Role::Option).named("Frances Lovelace #6"), true)?;
            canvas.click(by_role(Role::Option).named("Edsger Lovelace #3"))?;
            canvas.expect_toggled(by_role(Role::Option).named("Edsger Lovelace #3"), true)?;
            canvas.expect_toggled(by_role(Role::Option).named("Grace Lovelace #2"), false)?;
            canvas.expect_action("on_select", 1)?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Space))?;
            canvas.expect_toggled(by_role(Role::Option).named("Barbara Lovelace #4"), true)?;
            canvas.expect_toggled(by_role(Role::Option).named("Edsger Lovelace #3"), true)?;
            canvas.expect_action("on_selection_change", 2)?;
            canvas.expect_action("on_select", 1)
        }),
        preview!(virtual_list: VirtualListProps, "Wrapped rows", |p| {
            let list = virtual_list(
                VirtualListProps::props()
                    .items(messages())
                    .wrap(p.arg("wrap", true))
                    .selected(signal(Some(Arc::from("message-1"))))
                    .label(p.arg("label", "Messages"))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(framed(list)?)
        })
        .title("Layout/Virtual list")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::Option).named("Ada: Lunch? 9:00"))?;
            canvas.expect_toggled(by_role(Role::Option).named("Ada: Lunch? 9:00"), true)?;
            canvas.expect_action("on_select", 1)?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::ArrowDown))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_action("on_select", 2)
        }),
        preview!(virtual_list: VirtualListProps, "Empty", |p| {
            let list = virtual_list(
                VirtualListProps::props()
                    .label(p.arg("label", "Contacts"))
                    .empty_label(p.arg("empty_label", "No contacts yet"))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(framed(list)?)
        })
        .title("Layout/Virtual list")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.expect(by_role(Role::ListBox).named("Contacts"))?;
            canvas.expect_text("No contacts yet")?;
            canvas.expect_absent(by_role(Role::Option))
        }),
    ]
}
