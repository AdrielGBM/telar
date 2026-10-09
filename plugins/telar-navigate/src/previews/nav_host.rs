use telar::effect;
use telar::preview::{Layout, Matrix, PreviewEntry, preview};

use super::page::page;
use crate::{NavHost, NavTransition, Navigator};

#[derive(Clone, Copy, PartialEq, Eq, telar::PreviewArg)]
pub(super) enum Screen {
    Inbox,
    Message,
    Reply,
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(nav_host, "Stack", |p| {
            let screen = p.signal("screen", Screen::Inbox);
            let nav = Navigator::new(Screen::Inbox);
            effect(move || {
                let target = screen.get();
                match nav.peek_stack(|stack| stack.iter().position(|route| *route == target)) {
                    Some(index) => {
                        while nav.peek_stack(|stack| stack.len()) > index + 1 {
                            nav.pop();
                        }
                    }
                    None => nav.push(target),
                }
            });
            let mut host = NavHost::new(nav, |screen| match screen {
                Screen::Inbox => page("Inbox", "Three unread messages."),
                Screen::Message => page("Message", "Lunch on Friday?"),
                Screen::Reply => page("Reply", "Friday works for me."),
            })?;
            host.set_transition(p.arg("transition", NavTransition::SlideHorizontal));
            Ok::<_, telar::LayoutError>(host)
        })
        .title("Navigation/Nav host")
        .layout(Layout::Fullscreen)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
