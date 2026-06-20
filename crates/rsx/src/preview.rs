use crate::{
    App, AppConfig, Color, Component, Container, LayoutItem, LayoutStyle, PreviewEntry,
    ScrollablePage, Text, TextStyle, WidgetCtx,
};

pub struct PreviewApp {
    pub entries: Vec<PreviewEntry>,
}

impl App for PreviewApp {
    fn root(&self) -> Box<dyn Component> {
        let mut ctx = WidgetCtx::new();
        let mut sections: Vec<Box<dyn LayoutItem>> = Vec::new();

        for entry in &self.entries {
            let header_text = format!("[{}]  {}", entry.component_name, entry.preview_name);
            let header = Text::new(
                &mut ctx,
                move || header_text.clone(),
                LayoutStyle::new().padding_all(8.0),
                || TextStyle::new(11.0, Color::rgba(0.4, 0.4, 0.55, 1.0)),
            )
            .unwrap();

            let mut children: Vec<Box<dyn LayoutItem>> = vec![Box::new(header)];
            match (entry.build)(&mut ctx) {
                Ok(widget) => children.push(widget),
                Err(err) => {
                    let msg = format!("Error: {err}");
                    let label = Text::new(
                        &mut ctx,
                        move || msg.clone(),
                        LayoutStyle::new(),
                        || TextStyle::new(12.0, Color::rgba(0.9, 0.2, 0.2, 1.0)),
                    )
                    .unwrap();
                    children.push(Box::new(label));
                }
            }

            let section = Container::new(
                &mut ctx,
                LayoutStyle::new().flex_column().gap(8.0).padding_all(16.0),
                children,
            )
            .unwrap();
            sections.push(Box::new(section));
        }

        let content = Container::new(
            &mut ctx,
            LayoutStyle::new().flex_column().gap(16.0).padding_all(24.0),
            sections,
        )
        .unwrap();

        let page = ScrollablePage::new(ctx, Box::new(content), 0.0, 0.0);
        Box::new(page)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Color::rgba(0.96, 0.96, 0.98, 1.0))
    }
}

pub fn run_preview_window(entries: Vec<PreviewEntry>, config: AppConfig) {
    crate::run_app_with_name(config, PreviewApp { entries }, "rsx-preview");
}
