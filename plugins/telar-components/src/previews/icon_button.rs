use telar::Role;
use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};

use crate::icon_button::{IconButtonProps, icon_button};
use telar::ControlSize;

#[derive(Clone, Copy, telar::PreviewArg)]
enum Glyph {
    Search,
    Settings,
    Star,
    Copy,
}

impl Glyph {
    fn icon(self) -> telar::IconName {
        match self {
            Self::Search => telar::icon!("lucide:search"),
            Self::Settings => telar::icon!("lucide:settings"),
            Self::Star => telar::icon!("lucide:star"),
            Self::Copy => telar::icon!("lucide:copy"),
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
                    .size(p.arg("size", ControlSize::Regular))
                    .shortcut(p.arg("shortcut", "Mod+K"))
                    .disabled(p.arg("disabled", false))
                    .build(),
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
