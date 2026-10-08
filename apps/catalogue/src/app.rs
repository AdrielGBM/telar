use telar::{App, Color, ScrollPage, reset_layout_runtime};

use crate::theme::theme;

pub struct Root;

impl App for Root {
    fn root(&self) -> Box<dyn telar::Component> {
        reset_layout_runtime();
        let content = crate::home::home(
            crate::home::HomeProps::props().build(),
            telar::Children::default(),
        )
        .expect("home layout failed");
        Box::new(ScrollPage::new(content).expect("page layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(theme().surface_alt)
    }
}
