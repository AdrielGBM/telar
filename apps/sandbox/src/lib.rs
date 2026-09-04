//! Telar's documentation app: one section per feature, each rendered from its own `.rsx`.

// Module tree (core/, shared/) is auto-declared by `telar::app!` — see `auto_modules` in telar.toml.
telar::app!(
    core::theme::SandboxTheme,
    {
        core::theme::register_modes();
        // Open following the OS light/dark preference (modern for light, midnight for dark); the sidebar buttons still override manually until the next OS change.
        telar::follow_system(core::theme::DEFAULT_MODE, "midnight");
        telar::follow_locale_direction();
    },
    app_config(),
    core::app::SandboxRoot
);

/// The faces this app shapes its text in.
///
/// A browser build has to bring its own: there is no font directory behind a page, so a shaper handed nothing finds nothing and every string measures to zero. Every other target reads the system's.
fn app_config() -> telar::AppConfig {
    #[allow(unused_mut)]
    let mut config = telar::AppConfig::default();
    #[cfg(target_arch = "wasm32")]
    {
        config
            .font_data
            .push(include_bytes!("../assets/fonts/DejaVuSans.ttf").to_vec());
        config.font_family = Some("DejaVu Sans".to_string());
    }
    config
}

#[cfg(test)]
mod smoke {
    use telar::{App, AvailableSpace, Event, compute_layout};

    #[test]
    fn app_root_builds_and_lays_out() {
        telar::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = telar::ComponentList::new(crate::core::app::SandboxRoot.root());
        tree.on_event(&Event::WindowResized {
            width: 1200,
            height: 900,
        });
        let _ = tree.commands();
    }

    #[test]
    fn an_rtl_locale_mirrors_the_shell() {
        telar::set_theme(crate::core::theme::SandboxTheme::modern());
        telar::follow_locale_direction();
        telar::set_locale("en");
        let mut tree = telar::ComponentList::new(crate::core::app::SandboxRoot.root());
        let resize = Event::WindowResized {
            width: 1200,
            height: 900,
        };
        tree.on_event(&resize);
        let ltr = nav_rail_center_x(&tree);

        telar::set_locale("ar");
        tree.on_event(&resize);
        let rtl = nav_rail_center_x(&tree);

        telar::set_locale("en");
        assert!(
            ltr < 300.0,
            "the rail sits on the left under LTR, got {ltr}"
        );
        assert!(
            rtl > 900.0,
            "and on the right under RTL, got {rtl} (rail did not mirror)"
        );
    }

    /// The horizontal center of the widest sidebar-sized rect — the nav rail's own background.
    fn nav_rail_center_x(tree: &telar::ComponentList) -> f32 {
        let cmds = tree.commands();
        let mut widest: Option<(f32, f32)> = None;
        telar::for_each_with_matrix(&cmds, |c, m| {
            if let telar::DrawCommand::Rect { rect, .. } = c
                && rect.height > 700.0
                && rect.width < 400.0
            {
                let cx = m[4] + rect.x + rect.width / 2.0;
                if widest.is_none_or(|(w, _)| rect.width > w) {
                    widest = Some((rect.width, cx));
                }
            }
        });
        widest
            .expect("the desktop shell draws a full-height sidebar")
            .1
    }

    #[test]
    fn shell_survives_breakpoint_transition() {
        telar::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = telar::ComponentList::new(crate::core::app::SandboxRoot.root());
        for (width, height) in [(1200u32, 900u32), (380, 720), (1000, 800), (360, 640)] {
            tree.on_event(&Event::WindowResized { width, height });
            let _ = tree.commands();
        }
    }

    #[test]
    fn nav_click_switches_section() {
        use platform_core::{PointerButton, PointerSource};
        telar::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = telar::ComponentList::new(crate::core::app::SandboxRoot.root());
        tree.on_event(&Event::WindowResized {
            width: 1200,
            height: 900,
        });
        let before = tree.commands().to_vec();
        // Nav button 5 ("Boxes") sits ~36px below button 0 (≈y293) per button; click its center.
        for phase in [true, false] {
            let ev = if phase {
                Event::PointerPressed {
                    x: 110.0,
                    y: 489.0,
                    button: PointerButton::Primary,
                    source: PointerSource::Mouse,
                }
            } else {
                Event::PointerReleased {
                    x: 110.0,
                    y: 489.0,
                    button: PointerButton::Primary,
                    source: PointerSource::Mouse,
                }
            };
            tree.on_event(&ev);
        }
        let after = tree.commands().to_vec();
        assert!(
            before != after,
            "clicking a nav item should change the rendered output"
        );
    }

    // Regression for the reported theme-freeze: after navigating in one theme then switching, no nav button may stay frozen at a stale theme's colour. Replays the runner loop faithfully — the exact batch bracketing, dev force-ticks, hover moves and theme switches driven as real dispatched clicks.
    #[test]
    fn nav_tracks_theme_after_navigation() {
        use platform_core::{PointerButton, PointerSource};

        run_theme_tracking_scenario();

        fn run_theme_tracking_scenario() {
            // A nav button's composed rect: center point + fill color, walking the matrix stack.
            fn nav_rects(tree: &telar::ComponentList) -> Vec<(f32, f32, telar::Color)> {
                collect_button_rects(tree, |w, _cy| (190.0..230.0).contains(&w))
            }
            fn theme_button_centers(tree: &telar::ComponentList) -> Vec<(f32, f32)> {
                collect_button_rects(tree, |w, cy| (40.0..140.0).contains(&w) && cy < 220.0)
                    .into_iter()
                    .filter(|(_, _, c)| c.b > 0.5 && c.r < 0.6)
                    .map(|(cx, cy, _)| (cx, cy))
                    .collect()
            }
            fn collect_button_rects(
                tree: &telar::ComponentList,
                keep: impl Fn(f32, f32) -> bool,
            ) -> Vec<(f32, f32, telar::Color)> {
                let cmds = tree.commands();
                let mut out = Vec::new();
                telar::for_each_with_matrix(&cmds, |c, m| {
                    if let telar::DrawCommand::Rect { rect, style } = c {
                        let cy = m[5] + rect.y + rect.height / 2.0;
                        if (18.0..45.0).contains(&rect.height)
                            && keep(rect.width, cy)
                            && let Some(p) = style.fill.as_ref()
                        {
                            let cx = m[4] + rect.x + rect.width / 2.0;
                            out.push((cx, cy, p.solid_color()));
                        }
                    }
                });
                out
            }

            let feed = move |tree: &mut telar::ComponentList, ev: &Event| {
                telar::begin_batch();
                let _ = tree.on_event(ev);
                telar::end_batch();
            };
            let mv = |x: f32, y: f32| Event::PointerMoved {
                x: x as f64,
                y: y as f64,
                source: PointerSource::Mouse,
            };
            let pr = |x: f32, y: f32| Event::PointerPressed {
                x: x as f64,
                y: y as f64,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            };
            let rl = |x: f32, y: f32| Event::PointerReleased {
                x: x as f64,
                y: y as f64,
                button: PointerButton::Primary,
                source: PointerSource::Mouse,
            };

            telar::set_theme(crate::core::theme::SandboxTheme::modern());
            crate::core::theme::register_modes();
            // Tall enough that every nav button stays well inside the viewport. At 900 the deepest target sat 13px above the bottom edge, so a platform whose font metrics pushed the rail down dropped the click outside the window and left the previous section selected.
            const WINDOW_H: u32 = 1200;
            let mut tree = telar::ComponentList::new(crate::core::app::SandboxRoot.root());
            feed(
                &mut tree,
                &Event::WindowResized {
                    width: 1200,
                    height: WINDOW_H,
                },
            );
            assert!(nav_rects(&tree).len() >= 16, "need nav buttons laid out");

            let nav_to = |tree: &mut telar::ComponentList, target: usize| {
                let rects = nav_rects(tree);
                let (tx, ty, _) = rects[target];
                assert!(
                    ty < WINDOW_H as f32,
                    "nav[{target}] sits at y={ty:.0}, past the {WINDOW_H}px viewport — the click would never land"
                );
                for r in rects.iter().take(target + 1) {
                    feed(tree, &mv(r.0, r.1));
                }
                feed(tree, &pr(tx, ty));
                feed(tree, &rl(tx, ty));
                let _ = tree.commands();
            };
            let switch_theme = |tree: &mut telar::ComponentList, idx: usize| {
                let btns = theme_button_centers(tree);
                assert_eq!(btns.len(), 3, "expected 3 theme buttons, found {btns:?}");
                let (cx, cy) = btns[idx];
                feed(tree, &mv(cx, cy));
                feed(tree, &pr(cx, cy));
                feed(tree, &rl(cx, cy));
                let _ = tree.commands();
            };

            let close = |a: telar::Color, b: telar::Color| {
                (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
            };

            nav_to(&mut tree, 4);
            switch_theme(&mut tree, 1); // pastel
            nav_to(&mut tree, 8);
            switch_theme(&mut tree, 2); // midnight
            nav_to(&mut tree, 15);

            let themes = [
                (0usize, crate::core::theme::SandboxTheme::modern()),
                (1, crate::core::theme::SandboxTheme::pastel()),
                (2, crate::core::theme::SandboxTheme::midnight()),
            ];
            for (idx, theme) in themes {
                switch_theme(&mut tree, idx);
                let rects = nav_rects(&tree);
                let found = rects.len();
                for (i, (cx, cy, fill)) in rects.into_iter().enumerate() {
                    let want = if i == 15 {
                        theme.primary
                    } else {
                        theme.surface_alt
                    };
                    assert!(
                        close(fill, want),
                        "theme={} nav[{i}] of {found} at ({cx:.0},{cy:.0}) fill={:?} != expected {:?} (frozen at a stale theme)",
                        theme.name,
                        (fill.r, fill.g, fill.b),
                        (want.r, want.g, want.b),
                    );
                }
            }
        }
    }

    #[test]
    fn nav_highlight_follows_selection() {
        use platform_core::{PointerButton, PointerSource};
        // y-coordinates of primary-blue, nav-button-width (~208px) rects, composed through the matrix stack.
        fn active_nav_ys(tree: &telar::ComponentList) -> Vec<i32> {
            let cmds = tree.commands();
            let mut ys = Vec::new();
            telar::for_each_with_matrix(&cmds, |c, m| {
                if let telar::DrawCommand::Rect { rect, style } = c {
                    let blue = style
                        .fill
                        .as_ref()
                        .map(|p| p.solid_color())
                        .is_some_and(|c| c.b > 0.7 && c.r < 0.45);
                    if blue && rect.width > 150.0 && rect.width < 240.0 {
                        ys.push((m[5] + rect.y) as i32);
                    }
                }
            });
            ys
        }
        telar::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = telar::ComponentList::new(crate::core::app::SandboxRoot.root());
        tree.on_event(&Event::WindowResized {
            width: 1200,
            height: 900,
        });
        let initial = active_nav_ys(&tree);
        assert_eq!(
            initial.len(),
            1,
            "exactly one nav item highlighted initially: {initial:?}"
        );

        let mv = |x: f64, y: f64| Event::PointerMoved {
            x,
            y,
            source: PointerSource::Mouse,
        };
        let pr = |x: f64, y: f64| Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        };
        let rl = |x: f64, y: f64| Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        };
        for ev in [
            mv(110.0, 489.0),
            pr(110.0, 489.0),
            rl(110.0, 489.0),
            mv(600.0, 400.0),
        ] {
            tree.on_event(&ev);
        }
        let after = active_nav_ys(&tree);
        assert_eq!(
            after.len(),
            1,
            "exactly one nav item highlighted after navigating: {after:?}"
        );
        assert!(
            after[0] > initial[0],
            "highlight should move to the lower (later) section: {initial:?} -> {after:?}"
        );
    }

    // A hidden section using a Canvas that paints at fixed coordinates must not bleed its vector art over the visible one.
    #[test]
    fn hidden_canvas_sections_do_not_bleed() {
        telar::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = telar::ComponentList::new(crate::core::app::SandboxRoot.root());
        tree.on_event(&Event::WindowResized {
            width: 1200,
            height: 900,
        });
        let cmds = tree.commands();
        let mut leaked = 0;
        telar::for_each_with_matrix(&cmds, |c, cur| {
            if let telar::DrawCommand::Rect { rect, style } = c {
                let blue = style
                    .fill
                    .as_ref()
                    .map(|p| p.solid_color())
                    .is_some_and(|c| c.b > 0.7 && c.r < 0.45);
                let eff_x = cur[0] * rect.x + cur[2] * rect.y + cur[4];
                let eff_area = (rect.width * cur[0]).abs() * (rect.height * cur[3]).abs();
                if blue && eff_area > 1.0 && eff_x > 248.0 {
                    leaked += 1;
                }
            }
        });
        assert_eq!(
            leaked, 0,
            "hidden Canvas art leaked into the content: {leaked} blue rects"
        );
    }

    #[test]
    fn all_theme_variants_build() {
        for make in [
            crate::core::theme::SandboxTheme::modern as fn() -> crate::core::theme::SandboxTheme,
            crate::core::theme::SandboxTheme::pastel,
            crate::core::theme::SandboxTheme::midnight,
        ] {
            telar::set_theme(make());
            telar::reset_layout_runtime();
            let content = crate::features::color::color(
                crate::features::color::ColorProps::props().build(),
                telar::Children::default(),
            )
            .expect("color section builds");
            let node = telar::LayoutItem::layout_node(content.as_ref());
            compute_layout(
                node,
                AvailableSpace::Definite(800.0),
                AvailableSpace::MaxContent,
            )
            .expect("layout");
        }
    }
}
