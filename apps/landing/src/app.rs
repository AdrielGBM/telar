//! The landing page itself: the sections it is built from, laid out as one scrolling column.

use crate::theme::theme;
use telar::{App, Color, ScrollPage, reset_layout_runtime};

/// The page's root component.
pub struct LandingRoot;

impl App for LandingRoot {
    fn root(&self) -> Box<dyn telar::Component> {
        reset_layout_runtime();
        let content = crate::home::home(
            crate::home::HomeProps::props().build(),
            telar::Children::default(),
        )
        .expect("layout failed");
        let page = ScrollPage::new(content).expect("page layout failed");
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(theme().background)
    }
}
