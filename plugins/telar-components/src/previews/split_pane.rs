use telar::preview::{Layout, Matrix, PreviewEntry, by_role, by_text, preview};
use telar::{Children, Key, NamedKey, Role};

use super::sample;
use crate::split_pane::{SizedPane, SplitDirection, SplitPaneProps, split_pane};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(split_pane: SplitPaneProps, "Default", |p| {
            split_pane(
                SplitPaneProps::props()
                    .direction(p.arg("direction", SplitDirection::Row))
                    .sized(p.arg("sized", SizedPane::First))
                    .size(p.signal("size", 240.0f32))
                    .collapsed(p.signal("collapsed", false))
                    .min(p.arg("min", 120.0f32))
                    .max(p.arg("max", 0.0f32))
                    .collapsible(p.arg("collapsible", true))
                    .first(Children::one(|| sample::text("Sidebar")))
                    .second(Children::one(|| sample::text("Editor")))
                    .build(),
            )
        })
        .title("Workbench/Split pane")
        .layout(Layout::Fullscreen)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            let splitter = by_role(Role::Splitter);
            canvas.tab()?;
            canvas.expect_focused(splitter.clone())?;
            canvas.press(Key::Named(NamedKey::Home))?;
            canvas.expect_value(splitter.clone(), 120.0)?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_absent(by_text("Sidebar"))?;
            canvas.press(Key::Named(NamedKey::ArrowRight))?;
            canvas.expect(by_text("Sidebar"))?;
            canvas.expect_value(splitter, 120.0)
        }),
    ]
}
