// Module tree (core/, shared/) is auto-declared by `rsx::app!` — see `auto_modules` in rsx.toml.
rsx::app!(
    core::theme::SandboxTheme,
    {
        core::theme::register_modes();
        // Open following the OS light/dark preference (modern for light, midnight for dark); the sidebar
        // buttons still override manually until the next OS change.
        rsx::follow_system(core::theme::DEFAULT_MODE, "midnight");
    },
    rsx::AppConfig::default(),
    core::app::SandboxRoot
);

#[cfg(test)]
mod smoke {
    use rsx::{App, AvailableSpace, Event, compute_layout};

    // The whole documentation app must build and lay out without panicking, and every section fn
    // must produce a tree — a regression guard over the two-pane shell and all `.rsx` sections.
    #[test]
    fn app_root_builds_and_lays_out() {
        rsx::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = rsx::ComponentList::new(crate::core::app::SandboxRoot.root());
        tree.on_event(&Event::WindowResized {
            width: 1200,
            height: 900,
        });
        // Flatten once to make sure every section's view() runs.
        let _ = tree.commands();
    }

    // The responsive shell must survive both breakpoints and the transition between them: desktop rail,
    // mobile hamburger top bar, and back. A regression guard over the set_display collapse + drawer overlay.
    #[test]
    fn shell_survives_breakpoint_transition() {
        rsx::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = rsx::ComponentList::new(crate::core::app::SandboxRoot.root());
        for (width, height) in [(1200u32, 900u32), (380, 720), (1000, 800), (360, 640)] {
            tree.on_event(&Event::WindowResized { width, height });
            let _ = tree.commands();
        }
    }

    // Clicking a sidebar nav item switches the visible section — a guard over the whole
    // select → toggle-display → relayout path (desktop layout, so the sidebar rail is hit-testable).
    #[test]
    fn nav_click_switches_section() {
        use platform_core::{PointerButton, PointerSource};
        rsx::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = rsx::ComponentList::new(crate::core::app::SandboxRoot.root());
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

    // Regression for the reported theme-freeze: after navigating in one theme then switching themes, no
    // nav button may stay "frozen" at a stale theme's color. Replays the runner loop faithfully — the exact
    // batch bracketing (`new_events`/`about_to_wait`), dev force-ticks, hover moves, and theme switches driven
    // as real dispatched button clicks — then asserts every inactive nav button tracks the current theme's
    // `surface_alt` and the active one its `primary`, across the user's full modern→pastel→midnight sequence.
    #[test]
    fn nav_tracks_theme_after_navigation() {
        use platform_core::{PointerButton, PointerSource};
        use rsx::EventResult;

        // The runner only bumps force-ticks in dev builds. The freeze must not appear in either mode, so the
        // subscriptions themselves must stay intact; run the whole scenario under both to cover release too.
        for force_tick in [true, false] {
            run_theme_tracking_scenario(force_tick);
        }

        fn run_theme_tracking_scenario(force_tick: bool) {
            // A nav button's composed rect: center point + fill color, walking the matrix stack.
            fn nav_rects(tree: &rsx::ComponentList) -> Vec<(f32, f32, rsx::Color)> {
                collect_button_rects(tree, |w, _cy| (190.0..230.0).contains(&w))
            }
            // The three theme buttons: content-sized primary rects near the top of the sidebar.
            fn theme_button_centers(tree: &rsx::ComponentList) -> Vec<(f32, f32)> {
                collect_button_rects(tree, |w, cy| (40.0..140.0).contains(&w) && cy < 220.0)
                    .into_iter()
                    .filter(|(_, _, c)| c.b > 0.5 && c.r < 0.6)
                    .map(|(cx, cy, _)| (cx, cy))
                    .collect()
            }
            fn collect_button_rects(
                tree: &rsx::ComponentList,
                keep: impl Fn(f32, f32) -> bool,
            ) -> Vec<(f32, f32, rsx::Color)> {
                let cmds = tree.commands();
                let (mut tx, mut ty) = (vec![0.0f32], vec![0.0f32]);
                let mut out = Vec::new();
                for c in cmds.iter() {
                    match c {
                        rsx::DrawCommand::PushMatrix { matrix } => {
                            tx.push(tx.last().unwrap() + matrix[4]);
                            ty.push(ty.last().unwrap() + matrix[5]);
                        }
                        rsx::DrawCommand::PopMatrix => {
                            tx.pop();
                            ty.pop();
                        }
                        rsx::DrawCommand::Rect { rect, style } => {
                            let cy = ty.last().unwrap() + rect.y + rect.height / 2.0;
                            if (25.0..45.0).contains(&rect.height) && keep(rect.width, cy) {
                                if let Some(p) = style.fill.as_ref() {
                                    let cx = tx.last().unwrap() + rect.x + rect.width / 2.0;
                                    out.push((cx, cy, p.solid_color()));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                out
            }

            // One runner cycle for a single event: begin (new_events), dispatch, dev force-tick, end (flush).
            let feed = move |tree: &mut rsx::ComponentList, ev: &Event| {
                rsx::begin_batch();
                let handled = tree.on_event(ev);
                if force_tick && handled == EventResult::Handled {
                    tree.bump_force_ticks();
                }
                rsx::end_batch();
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

            rsx::set_theme(crate::core::theme::SandboxTheme::modern());
            // The sidebar theme buttons now call `set_mode(id)`; register the appliers so a click installs the variant.
            crate::core::theme::register_modes();
            let mut tree = rsx::ComponentList::new(crate::core::app::SandboxRoot.root());
            feed(
                &mut tree,
                &Event::WindowResized {
                    width: 1200,
                    height: 900,
                },
            );
            assert!(nav_rects(&tree).len() >= 16, "need nav buttons laid out");

            // Navigate to `target`: mouse travels down over the buttons above it (hover churn), then clicks it.
            let nav_to = |tree: &mut rsx::ComponentList, target: usize| {
                let rects = nav_rects(tree);
                let (tx, ty, _) = rects[target];
                for r in rects.iter().take(target + 1) {
                    feed(tree, &mv(r.0, r.1));
                }
                feed(tree, &pr(tx, ty));
                feed(tree, &rl(tx, ty));
                let _ = tree.commands();
            };
            // Switch theme by clicking its button (dispatched event — button borrowed mid-dispatch, the real path).
            let switch_theme = |tree: &mut rsx::ComponentList, idx: usize| {
                let btns = theme_button_centers(tree);
                assert_eq!(btns.len(), 3, "expected 3 theme buttons, found {btns:?}");
                let (cx, cy) = btns[idx];
                feed(tree, &mv(cx, cy));
                feed(tree, &pr(cx, cy));
                feed(tree, &rl(cx, cy));
                let _ = tree.commands();
            };

            let close = |a: rsx::Color, b: rsx::Color| {
                (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
            };

            // The user's exact sequence: modern→click4, pastel→click8, midnight→click15.
            nav_to(&mut tree, 4);
            switch_theme(&mut tree, 1); // pastel
            nav_to(&mut tree, 8);
            switch_theme(&mut tree, 2); // midnight
            nav_to(&mut tree, 15);

            // Now sweep the themes: every inactive nav button must show the CURRENT theme's surface_alt, and
            // the active one (idx 15) its primary — none frozen at a stale theme.
            let themes = [
                (0usize, crate::core::theme::SandboxTheme::modern()),
                (1, crate::core::theme::SandboxTheme::pastel()),
                (2, crate::core::theme::SandboxTheme::midnight()),
            ];
            for (idx, theme) in themes {
                switch_theme(&mut tree, idx);
                for (i, (_, _, fill)) in nav_rects(&tree).into_iter().enumerate() {
                    let want = if i == 15 {
                        theme.primary
                    } else {
                        theme.surface_alt
                    };
                    assert!(
                        close(fill, want),
                        "force_tick={force_tick} theme={} nav[{i}] fill={:?} != expected {:?} (frozen at a stale theme)",
                        theme.name,
                        (fill.r, fill.g, fill.b),
                        (want.r, want.g, want.b),
                    );
                }
            }
        }
    }

    // The active sidebar item is highlighted (one primary-blue nav button) and the highlight follows the
    // selection — verified through the real interaction sequence (hover, press, release, move away).
    #[test]
    fn nav_highlight_follows_selection() {
        use platform_core::{PointerButton, PointerSource};
        // y-coordinates of primary-blue, nav-button-width (~208px) rects, composed through the matrix stack.
        fn active_nav_ys(tree: &rsx::ComponentList) -> Vec<i32> {
            let cmds = tree.commands();
            let mut ty = vec![0.0f32];
            let mut ys = Vec::new();
            for c in cmds.iter() {
                match c {
                    rsx::DrawCommand::PushMatrix { matrix } => {
                        ty.push(ty.last().unwrap() + matrix[5])
                    }
                    rsx::DrawCommand::PopMatrix => {
                        ty.pop();
                    }
                    rsx::DrawCommand::Rect { rect, style } => {
                        let blue = style
                            .fill
                            .as_ref()
                            .map(|p| p.solid_color())
                            .is_some_and(|c| c.b > 0.7 && c.r < 0.45);
                        if blue && rect.width > 150.0 && rect.width < 240.0 {
                            ys.push((ty.last().unwrap() + rect.y) as i32);
                        }
                    }
                    _ => {}
                }
            }
            ys
        }
        rsx::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = rsx::ComponentList::new(crate::core::app::SandboxRoot.root());
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

    // A hidden section (Transforms/Paths/Motion use a Canvas that paints at fixed coordinates) must not
    // bleed its vector art over the visible section. After building, only one primary-blue rect from a
    // Canvas-free source should sit in the content region; the strays are gone.
    #[test]
    fn hidden_canvas_sections_do_not_bleed() {
        rsx::set_theme(crate::core::theme::SandboxTheme::modern());
        let mut tree = rsx::ComponentList::new(crate::core::app::SandboxRoot.root());
        tree.on_event(&Event::WindowResized {
            width: 1200,
            height: 900,
        });
        // Overview (the initial section) has no primary-blue fills; any blue rect that composes to the
        // content region (effective x ≳ 248) with a non-zero size is leaked Canvas art from a hidden section.
        fn compose(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
            [
                a[0] * b[0] + a[2] * b[1],
                a[1] * b[0] + a[3] * b[1],
                a[0] * b[2] + a[2] * b[3],
                a[1] * b[2] + a[3] * b[3],
                a[0] * b[4] + a[2] * b[5] + a[4],
                a[1] * b[4] + a[3] * b[5] + a[5],
            ]
        }
        let cmds = tree.commands();
        let mut cur = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
        let mut stack: Vec<[f32; 6]> = Vec::new();
        let mut leaked = 0;
        for c in cmds.iter() {
            match c {
                rsx::DrawCommand::PushMatrix { matrix } => {
                    stack.push(cur);
                    cur = compose(cur, *matrix);
                }
                rsx::DrawCommand::PopMatrix => {
                    cur = stack.pop().unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
                }
                rsx::DrawCommand::Rect { rect, style } => {
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
                _ => {}
            }
        }
        assert_eq!(
            leaked, 0,
            "hidden Canvas art leaked into the content: {leaked} blue rects"
        );
    }

    // Each themed variant must resolve without a missing-token panic.
    #[test]
    fn all_theme_variants_build() {
        for make in [
            crate::core::theme::SandboxTheme::modern as fn() -> crate::core::theme::SandboxTheme,
            crate::core::theme::SandboxTheme::pastel,
            crate::core::theme::SandboxTheme::midnight,
        ] {
            rsx::set_theme(make());
            rsx::reset_layout_runtime();
            let content = crate::features_color().expect("color section builds");
            let node = rsx::LayoutItem::layout_node(content.as_ref());
            compute_layout(
                node,
                AvailableSpace::Definite(800.0),
                AvailableSpace::MaxContent,
            )
            .expect("layout");
        }
    }
}
