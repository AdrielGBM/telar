pub mod app;
pub mod demo_images;
pub mod demo_svgs;
pub mod theme;

rsx::app!(
    theme::SandboxTheme,
    {
        rsx::set_theme_with_widgets(theme::SandboxTheme::modern());
    },
    rsx::AppConfig::default(),
    app::SandboxRoot
);

// Headless regression test for the Motion section: the canvas draw closure gets a LOCAL rect (a past double-offset drew the spring square over the toggle button), and the opacity transition must tween — not jump — after a real button press.
#[cfg(test)]
mod motion_section_test {
    use platform_core::{Event, PointerButton, PointerSource};
    use rsx::{AvailableSpace, ComponentList, DrawCommand, WidgetCtx, compute_layout};

    fn layer_opacity(cmds: &[DrawCommand]) -> Option<f32> {
        cmds.iter().find_map(|c| match c {
            DrawCommand::PushLayer { opacity, .. } => Some(*opacity),
            _ => None,
        })
    }

    #[test]
    fn toggle_tweens_opacity_and_keeps_spring_square_in_its_slot() {
        rsx::set_theme_with_widgets(crate::theme::SandboxTheme::modern());
        let mut ctx = WidgetCtx::new();
        let item = crate::sections_motion_section(&mut ctx).expect("build");
        let node = rsx::LayoutItem::layout_node(item.as_ref());
        compute_layout(
            &mut ctx,
            node,
            AvailableSpace::Definite(800.0),
            AvailableSpace::Definite(600.0),
        )
        .expect("layout");
        let mut tree = ComponentList::new(item);

        // The spring square must sit inside the canvas's local 100x100 slot; the double-offset bug drew it at the absolute layout position AGAIN (over the toggle button).
        let (button_center, square_local) = {
            let cmds = tree.commands();
            let mut button = None;
            let mut square = None;
            let mut tx = 0.0f32;
            let mut ty = 0.0f32;
            for c in cmds.iter() {
                match c {
                    DrawCommand::PushMatrix { matrix } => {
                        tx = matrix[4];
                        ty = matrix[5];
                    }
                    DrawCommand::Rect { rect, .. } => {
                        if (rect.width - 60.0).abs() < 0.1 && (rect.height - 60.0).abs() < 0.1 {
                            square = Some((rect.x, rect.y));
                        } else if rect.height > 15.0 && rect.height < 55.0 && rect.width > 30.0 {
                            button = Some((
                                tx + rect.x + rect.width / 2.0,
                                ty + rect.y + rect.height / 2.0,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            (button.expect("button rect"), square.expect("spring square"))
        };
        assert_eq!(
            square_local,
            (20.0, 20.0),
            "canvas closure must receive a zero-origin rect"
        );
        assert!(
            layer_opacity(&tree.commands()).is_none(),
            "fully opaque box must not push a layer"
        );

        tree.on_event(&Event::PointerPressed {
            x: button_center.0 as f64,
            y: button_center.1 as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        tree.on_event(&Event::PointerReleased {
            x: button_center.0 as f64,
            y: button_center.1 as f64,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });

        let t0 = std::time::Instant::now();
        rsx::motion::tick(t0);
        rsx::motion::tick(t0 + std::time::Duration::from_millis(150));
        let mid = layer_opacity(&tree.commands()).expect("layer during fade");
        assert!(
            mid > 0.2 && mid < 0.9,
            "opacity must be mid-tween at 150ms of a 300ms fade, got {mid}"
        );
        rsx::motion::tick(t0 + std::time::Duration::from_millis(400));
        let settled = layer_opacity(&tree.commands()).expect("layer while dimmed");
        assert!(
            (settled - 0.15).abs() < 0.01,
            "fade settles at 0.15, got {settled}"
        );
    }
}
