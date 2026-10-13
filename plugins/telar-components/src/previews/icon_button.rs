use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use telar::{Children, Role};
use telar_icons::IconName;

use crate::icon_button::{IconButtonProps, icon_button};

#[derive(Clone, Copy, telar::PreviewArg)]
enum Glyph {
    Search,
    Settings,
    Star,
    Copy,
}

impl Glyph {
    fn icon(self) -> IconName {
        match self {
            Self::Search => telar_icons::icon!("lucide:search"),
            Self::Settings => telar_icons::icon!("lucide:settings"),
            Self::Star => telar_icons::icon!("lucide:star"),
            Self::Copy => telar_icons::icon!("lucide:copy"),
        }
    }
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(icon_button: IconButtonProps, "Default", |p| {
            icon_button(
                IconButtonProps::props()
                    .icon(p.arg("icon", Glyph::Search).icon())
                    .label(p.arg("label", "Search"))
                    .shortcut(p.arg("shortcut", "Mod+K"))
                    .disabled(p.arg("disabled", false))
                    .build(),
                Children::default(),
            )
        })
        .title("Workbench/Icon button")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.click(by_role(Role::Button).named("Search"))?;
            canvas.expect_action("on_press", 1)
        }),
        preview!(icon_button: IconButtonProps, "Toggle", |p| {
            let pressed = p.signal("pressed", false);
            icon_button(
                IconButtonProps::props()
                    .icon(Glyph::Star.icon())
                    .label("Favourite")
                    .pressed(pressed)
                    .on_press(std::rc::Rc::new(move || pressed.update(|on| *on = !*on)))
                    .build(),
                Children::default(),
            )
        })
        .title("Workbench/Icon button")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .play(|canvas| {
            let favourite = by_role(Role::Button).named("Favourite");
            canvas.click(favourite.clone())?;
            canvas.expect_toggled(favourite.clone(), true)?;
            canvas.click(favourite.clone())?;
            canvas.expect_toggled(favourite, false)
        }),
    ]
}
