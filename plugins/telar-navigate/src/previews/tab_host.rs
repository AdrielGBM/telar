use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use super::page::page;
use crate::{NavTransition, Navigator, TabHost, TabStacks};

#[derive(Clone, Copy, PartialEq, Eq, telar::PreviewArg)]
enum Tab {
    Home,
    Search,
    Profile,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Root;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(tab_host, "Tabs", |p| {
            let stacks = TabStacks::new(
                p.signal("tab", Tab::Home),
                &[Tab::Home, Tab::Search, Tab::Profile],
                |_| Navigator::new(Root),
            );
            let host = TabHost::new(stacks, |tab, _| match tab {
                Tab::Home => page("Home", "What changed since you were last here."),
                Tab::Search => page("Search", "Find people, files and messages."),
                Tab::Profile => page("Profile", "Your name, photo and status."),
            })?
            .with_tab_transition(p.arg("tab_transition", NavTransition::Fade));
            Ok::<_, telar::LayoutError>(host)
        })
        .title("Navigation/Tab host")
        .layout(Layout::Fullscreen)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
