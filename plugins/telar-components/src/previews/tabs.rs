use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Color};

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
                Children::default(),
            )
        })
        .title("Navigation/Tabs")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(tabs: TabsProps, "Bound selection", |p| {
            let selected = p.signal("selected", 0u32);
            tabs(
                TabsProps::props().items(labels(4)).selected(selected).build(),
                Children::default(),
            )
        })
        .title("Navigation/Tabs")
        .layout(Layout::Centered)
        .tags(&["stateful"]),
    ]
}
