use telar::preview::{Layout, Matrix, PreviewEntry, by_role, by_text, preview};
use telar::{Children, Key, NamedKey, Role};

use super::sample::{self, Given};
use crate::workbench::split_pane::split_pane::{
    PaneLimit, SizedPane, SplitDirection, SplitPaneProps, split_pane,
};

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(split_pane: SplitPaneProps, "Default", |p| {
            split_pane(
                SplitPaneProps::props()
                    .direction(p.arg("direction", SplitDirection::Row))
                    .sized(p.arg("sized", SizedPane::First))
                    .size(p.signal("size", 240.0f32))
                    .collapsed(p.signal("collapsed", false))
                    .min(PaneLimit::Px(p.arg("min_px", 120.0f32)))
                    .given(p.arg("max_px", None::<f32>), |props, max| props.max(PaneLimit::Px(max)))
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
