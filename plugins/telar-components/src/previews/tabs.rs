use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Color, Role};

use crate::tabs::{TabsProps, tabs};

const LABELS: [&str; 6] = [
    "Overview", "Activity", "Settings", "Billing", "Members", "Audit",
];

fn labels(count: u32) -> Vec<&'static str> {
    LABELS.iter().copied().take(count as usize).collect()
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(tabs: TabsProps, "Default", |p| {
            tabs(
                TabsProps::props()
                    .items(labels(p.arg("count", 3u32)))
                    .color(p.arg("color", Color::TRANSPARENT))
                    .build(),
            )
        })
        .title("Navigation/Tabs")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.click(by_role(Role::Tab).named("Activity"))?;
            canvas.expect_toggled(by_role(Role::Tab).named("Activity"), true)?;
            canvas.expect_toggled(by_role(Role::Tab).named("Overview"), false)
        }),
        preview!(tabs: TabsProps, "Bound selection", |p| {
            let selected = p.signal("selected", 0u32);
            tabs(
                TabsProps::props()
                    .items(labels(4))
                    .selected(selected)
                    .build(),
            )
        })
        .title("Navigation/Tabs")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::Tab).named("Billing"))?;
            canvas.expect_toggled(by_role(Role::Tab).named("Billing"), true)?;
            canvas.click(by_role(Role::Tab).named("Settings"))?;
            canvas.expect_toggled(by_role(Role::Tab).named("Settings"), true)?;
            canvas.expect_toggled(by_role(Role::Tab).named("Billing"), false)
        }),
    ]
}
