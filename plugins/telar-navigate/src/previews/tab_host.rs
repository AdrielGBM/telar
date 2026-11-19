use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Container, LayoutError, LayoutItem, LayoutStyle, Text, box_item, effect};

use super::page::page;
use crate::{NavPage, NavTransition, Navigator, TabHost, TabStacks};

#[derive(Clone, Copy, Debug, PartialEq, Eq, telar::PreviewArg)]
pub(super) enum Tab {
    Home,
    Search,
    Profile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, telar::PreviewArg)]
pub(super) enum Screen {
    Root,
    Draft,
}

const TABS: [Tab; 3] = [Tab::Home, Tab::Search, Tab::Profile];

fn tab_page(tab: &Tab, screen: &Screen) -> Result<Box<dyn NavPage>, LayoutError> {
    match (tab, screen) {
        (Tab::Home, Screen::Root) => page("Home", "What changed since you were last here."),
        (Tab::Home, Screen::Draft) => page("Draft", "Notes for Friday's meeting."),
        (Tab::Search, _) => page("Search", "Find people, files and messages."),
        (Tab::Profile, _) => page("Profile", "Your name, photo and status."),
    }
}

/// A side panel beside the tab's pages, which stays as the pages beside it change.
fn beside_panel(pages: Children) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let heading = Text::declaring(|| "Recent".to_string(), LayoutStyle::new(), |t| t)?;
    let entry = Text::declaring(|| "Friday".to_string(), LayoutStyle::new(), |t| t)?;
    let panel = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(8.0)
            .padding_all(16.0)
            .width(160.0)
            .flex_shrink(0.0),
        vec![box_item(heading), box_item(entry)],
    )?;
    let mut items = vec![box_item(panel)];
    items.extend(pages.build()?);
    Ok(box_item(Container::new(
        LayoutStyle::new().flex_row().flex_grow(1.0),
        items,
    )?))
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(tab_host, "Tabs", |p| {
            let stacks = TabStacks::new(p.signal("tab", Tab::Home), &TABS, |_| {
                Navigator::new(Screen::Root)
            });
            let host = TabHost::new(stacks, |tab, _| match tab {
                Tab::Home => page("Home", "What changed since you were last here."),
                Tab::Search => page("Search", "Find people, files and messages."),
                Tab::Profile => page("Profile", "Your name, photo and status."),
            })?
            .with_tab_transition(p.arg("tab_transition", NavTransition::Fade));
            Ok::<_, LayoutError>(host)
        })
        .title("Navigation/Tab host")
        .layout(Layout::Fullscreen)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
        preview!(tab_host, "Framed tab", |p| {
            let stacks = TabStacks::new(p.signal("tab", Tab::Home), &TABS, |_| {
                Navigator::new(Screen::Root)
            });
            let home = stacks.navigator_for(&Tab::Home).expect("Home is declared");
            let screen = p.signal("screen", Screen::Root);
            effect(move || match screen.get() {
                Screen::Draft if home.peek_stack(|stack| stack.len()) == 1 => {
                    home.push(Screen::Draft)
                }
                Screen::Draft => {}
                Screen::Root => home.pop_to_root(),
            });
            let host = TabHost::new(stacks, tab_page)?.with_frame(Tab::Home, beside_panel)?;
            Ok::<_, LayoutError>(host)
        })
        .title("Navigation/Tab host")
        .layout(Layout::Fullscreen)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.expect_text("Recent")?;
            canvas.expect_text("What changed since you were last here.")?;
            canvas
                .ctx()
                .signal("screen", Screen::Root)
                .set(Screen::Draft);
            canvas.settle()?;
            canvas.expect_text("Notes for Friday's meeting.")?;
            canvas.expect_text("Recent")?;
            canvas.ctx().signal("tab", Tab::Home).set(Tab::Search);
            canvas.settle()?;
            canvas.expect_text("Find people, files and messages.")?;
            canvas.expect_no_text("Recent")?;
            canvas.ctx().signal("tab", Tab::Home).set(Tab::Home);
            canvas.settle()?;
            canvas.expect_text("Recent")?;
            canvas.expect_text("Notes for Friday's meeting.")
        }),
    ]
}
