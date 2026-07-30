pub mod app;
pub mod demo_images;
pub mod theme;

telar::app!(
    theme::LandingTheme,
    {
        telar::set_theme(theme::LandingTheme::light());
    },
    telar::AppConfig::default(),
    app::LandingRoot
);

#[cfg(test)]
mod layout_tests {
    use telar::{AvailableSpace, LayoutItem, compute_layout, track_layout};

    // The root page must fill the available window width at any size so its full-bleed bands stretch instead of collapsing into a centered column.
    fn page_rect_at(window_width: f32) -> (f32, f32) {
        telar::set_theme(crate::theme::LandingTheme::light());
        telar::reset_layout_runtime();
        let page = crate::home().expect("home build");
        let node = page.layout_node();
        compute_layout(
            node,
            AvailableSpace::Definite(window_width),
            AvailableSpace::MaxContent,
        )
        .expect("layout");
        let r = track_layout(node).expect("tracked").get();
        (r.width, r.height)
    }

    #[test]
    fn page_fills_wide_window() {
        assert!((page_rect_at(1400.0).0 - 1400.0).abs() < 1.0);
    }

    #[test]
    fn page_fills_narrow_window() {
        assert!((page_rect_at(480.0).0 - 480.0).abs() < 1.0);
    }

    // Narrowing the window past the split breakpoints should wrap the side-by-side sections into stacked columns, making the page taller — i.e. it reflows.
    #[test]
    fn narrow_window_reflows_taller() {
        let wide = page_rect_at(1400.0).1;
        let narrow = page_rect_at(480.0).1;
        assert!(
            narrow > wide,
            "expected narrow ({narrow}) taller than wide ({wide})"
        );
    }

    // Walks the flattened draw commands applying the PushMatrix/PopMatrix stack and returns the rightmost edge of actually-drawn content (Rect/Text/Image). This is what the user sees, unlike the page node rect.
    fn content_right_edge(cmds: &[telar::DrawCommand]) -> f32 {
        use telar::DrawCommand::*;
        fn mul(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
            [
                a[0] * b[0] + a[2] * b[1],
                a[1] * b[0] + a[3] * b[1],
                a[0] * b[2] + a[2] * b[3],
                a[1] * b[2] + a[3] * b[3],
                a[0] * b[4] + a[2] * b[5] + a[4],
                a[1] * b[4] + a[3] * b[5] + a[5],
            ]
        }
        let ident = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let mut stack = vec![ident];
        let mut max_x = 0.0f32;
        for c in cmds {
            match c {
                PushMatrix { matrix } => {
                    let top = *stack.last().unwrap();
                    stack.push(mul(top, *matrix));
                }
                PopMatrix => {
                    stack.pop();
                }
                Rect { rect, .. } | Text { rect, .. } | Image { rect, .. } => {
                    let m = stack.last().unwrap();
                    let right = rect.x + rect.width;
                    // apply affine to the rect's right edge (y irrelevant for max-x here)
                    let x = m[0] * right + m[2] * rect.y + m[4];
                    if x > max_x {
                        max_x = x;
                    }
                }
                _ => {}
            }
        }
        max_x
    }

    // Auto-height text must reserve more vertical space when it is narrower (more wrapped lines), so following content is pushed down instead of overlapped.
    #[test]
    fn auto_text_height_grows_when_narrower() {
        use telar::{
            AvailableSpace, Color, LayoutItem, LayoutStyle, Text, TextStyle, compute_layout,
            track_layout,
        };
        let long = "This is a deliberately long paragraph of text that wraps onto several \
                    lines when the available width is small, and fewer lines when it is wide.";
        let height_at = |w: f32| -> f32 {
            telar::reset_layout_runtime();
            let t = Text::auto(
                move || long.to_string(),
                LayoutStyle::new(),
                || TextStyle::new(16.0, Color::BLACK),
            )
            .unwrap();
            let node = t.layout_node();
            compute_layout(
                node,
                AvailableSpace::Definite(w),
                AvailableSpace::MaxContent,
            )
            .unwrap();
            track_layout(node).unwrap().get().height
        };
        let narrow = height_at(200.0);
        let wide = height_at(800.0);
        assert!(
            narrow > wide + 20.0,
            "narrow text should be taller: narrow={narrow} wide={wide}"
        );
    }

    // The real feature cards (different body lengths → different heights) in a wrap row must not overflow the row's reserved height when one wraps to line 2.
    #[test]
    fn real_feature_cards_do_not_overflow_row() {
        use telar::{
            AvailableSpace, LayoutItem, LayoutStyle, compute_layout, new_container, new_leaf,
            track_layout,
        };
        telar::set_theme(crate::theme::LandingTheme::light());
        telar::reset_layout_runtime();
        let bodies = [
            (
                "⚡",
                "Fast",
                "Software and wgpu renderers with dirty-tracking and scroll-blit detection.",
            ),
            (
                "🧩",
                "Composable",
                "Signals, memos and reusable .rsx components compose right inside the markup.",
            ),
            (
                "🎨",
                "Themeable",
                "Semantic color tokens resolve reactively, so dark mode is a single swap.",
            ),
            (
                "📱",
                "Cross-platform",
                "One codebase targets desktop and Android with native event loops.",
            ),
        ];
        let cards: Vec<Box<dyn LayoutItem>> = bodies
            .iter()
            .map(|(icon, title, body)| {
                crate::feature_card(crate::FeatureCardProps { icon, title, body }).unwrap()
            })
            .collect();
        let card_nodes: Vec<_> = cards.iter().map(|c| c.layout_node()).collect();
        let row = new_container(
            LayoutStyle::new().flex_row().flex_wrap().gap(24.0),
            &card_nodes,
        )
        .unwrap();
        let (marker, _) = new_leaf(LayoutStyle::new().height(50.0)).unwrap();
        let col =
            new_container(LayoutStyle::new().flex_column().gap(20.0), &[row, marker]).unwrap();
        // 1120 = the centered content width on a large screen → 3 cards + 1 on line 2.
        compute_layout(
            col,
            AvailableSpace::Definite(1120.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let row_rect = track_layout(row).unwrap().get();
        let row_bottom = row_rect.y + row_rect.height;
        for (i, node) in card_nodes.iter().enumerate() {
            let cr = track_layout(*node).unwrap().get();
            assert!(
                cr.y + cr.height <= row_bottom + 0.5,
                "card {i} overflows row: card_bottom={} row_bottom={row_bottom}",
                cr.y + cr.height
            );
        }
        let marker_rect = track_layout(marker).unwrap().get();
        assert!(
            marker_rect.y >= row_bottom - 0.5,
            "marker overlaps row: marker.y={} row_bottom={row_bottom}",
            marker_rect.y
        );
    }

    // A wrapping flex row must reserve height for ALL its lines so a following sibling sits below it instead of overlapping. Reproduces the "next section positions as if the wrapped card didn't exist" report.
    #[test]
    fn wrapped_flex_row_reserves_height_for_all_lines() {
        use telar::{
            AvailableSpace, LayoutStyle, compute_layout, new_container, new_leaf, track_layout,
        };
        telar::reset_layout_runtime();
        let mut cards = Vec::new();
        for _ in 0..4 {
            let (n, _) = new_leaf(
                LayoutStyle::new()
                    .min_width(260.0)
                    .height(100.0)
                    .flex_grow(1.0),
            )
            .unwrap();
            cards.push(n);
        }
        let row =
            new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &cards).unwrap();
        let (marker, _) = new_leaf(LayoutStyle::new().height(50.0)).unwrap();
        let col =
            new_container(LayoutStyle::new().flex_column().gap(20.0), &[row, marker]).unwrap();
        // 900px wide → 3 cards on line 1, the 4th wraps to line 2.
        compute_layout(
            col,
            AvailableSpace::Definite(900.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let row_rect = track_layout(row).unwrap().get();
        let marker_rect = track_layout(marker).unwrap().get();
        assert!(
            row_rect.height >= 220.0,
            "wrapped row height {} should cover 2 lines (~224)",
            row_rect.height
        );
        assert!(
            marker_rect.y >= row_rect.y + row_rect.height - 0.5,
            "marker overlaps wrapped row: row.y={} row.h={} marker.y={}",
            row_rect.y,
            row_rect.height,
            marker_rect.y
        );
    }

    // Same as above but the cards are content-sized containers (a column whose height comes from its children) with grow:1 — the real feature-card shape.
    #[test]
    fn wrapped_content_sized_cards_reserve_height() {
        use telar::{
            AvailableSpace, LayoutStyle, compute_layout, new_container, new_leaf, track_layout,
        };
        telar::reset_layout_runtime();
        let mut cards = Vec::new();
        for _ in 0..4 {
            let (inner, _) = new_leaf(LayoutStyle::new().height(100.0)).unwrap();
            let card = new_container(
                LayoutStyle::new()
                    .flex_column()
                    .min_width(260.0)
                    .flex_grow(1.0),
                &[inner],
            )
            .unwrap();
            cards.push(card);
        }
        let row =
            new_container(LayoutStyle::new().flex_row().flex_wrap().gap(24.0), &cards).unwrap();
        let (marker, _) = new_leaf(LayoutStyle::new().height(50.0)).unwrap();
        let col =
            new_container(LayoutStyle::new().flex_column().gap(20.0), &[row, marker]).unwrap();
        compute_layout(
            col,
            AvailableSpace::Definite(900.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let row_rect = track_layout(row).unwrap().get();
        let marker_rect = track_layout(marker).unwrap().get();
        assert!(
            row_rect.height >= 220.0,
            "wrapped content-sized row height {} should cover 2 lines (~224)",
            row_rect.height
        );
        assert!(
            marker_rect.y >= row_rect.y + row_rect.height - 0.5,
            "marker overlaps row: row.y={} row.h={} marker.y={}",
            row_rect.y,
            row_rect.height,
            marker_rect.y
        );
    }

    fn content_bottom_edge(cmds: &[telar::DrawCommand]) -> f32 {
        use telar::DrawCommand::*;
        let mut ty = 0.0f32;
        let mut stack = vec![0.0f32];
        let mut max_y = 0.0f32;
        for c in cmds {
            match c {
                PushMatrix { matrix } => {
                    ty += matrix[5];
                    stack.push(ty);
                }
                PopMatrix => {
                    stack.pop();
                    ty = *stack.last().unwrap();
                }
                Rect { rect, .. } | Text { rect, .. } | Image { rect, .. } => {
                    let bottom = ty + rect.y + rect.height;
                    if bottom > max_y {
                        max_y = bottom;
                    }
                }
                _ => {}
            }
        }
        max_y
    }

    // Diagnostic: at a wrap-inducing width, the drawn content must not extend below the page's own computed height — otherwise text/cards overflow onto whatever sits below them (the "next section positions as if it didn't exist" report).
    #[test]
    fn drawn_content_stays_within_page_height() {
        use telar::{App, AvailableSpace, Event, LayoutItem, compute_layout, track_layout};
        let bottom = {
            telar::set_theme(crate::theme::LandingTheme::light());
            let mut tree = telar::ComponentList::new(crate::app::LandingRoot.root());
            tree.on_event(&Event::WindowResized {
                width: 1000,
                height: 800,
            });
            let cmds = tree.commands();
            content_bottom_edge(&cmds)
        };

        telar::set_theme(crate::theme::LandingTheme::light());
        telar::reset_layout_runtime();
        let page = crate::home().unwrap();
        let node = page.layout_node();
        compute_layout(
            node,
            AvailableSpace::Definite(1000.0),
            AvailableSpace::MaxContent,
        )
        .unwrap();
        let page_h = track_layout(node).unwrap().get().height;
        println!(
            "page_height={page_h} drawn_bottom={bottom} overflow={}",
            bottom - page_h
        );
        assert!(
            bottom <= page_h + 2.0,
            "content overflows page by {}px (page={page_h}, drawn_bottom={bottom})",
            bottom - page_h
        );
    }

    // Collects (top, bottom) of full-width band background rects in absolute coords.
    fn collect_bands(cmds: &[telar::DrawCommand], min_width: f32) -> Vec<(f32, f32)> {
        use telar::DrawCommand::*;
        let mut ty = 0.0f32;
        let mut stack = vec![0.0f32];
        let mut out = Vec::new();
        for c in cmds {
            match c {
                PushMatrix { matrix } => {
                    ty += matrix[5];
                    stack.push(ty);
                }
                PopMatrix => {
                    stack.pop();
                    ty = *stack.last().unwrap();
                }
                Rect { rect, .. } if rect.width >= min_width => {
                    out.push((ty + rect.y, ty + rect.y + rect.height));
                }
                _ => {}
            }
        }
        out
    }

    // Detects the real symptom: content drawn BEFORE a later full-width band that falls inside that band's vertical range — the band, painted afterwards, covers it ("cross-platform card stays behind Built-in rendering").
    #[test]
    fn no_content_hidden_behind_a_later_band() {
        use telar::{App, DrawCommand::*, Event};
        for w in [560u32, 720, 900, 1100, 1280, 1440, 1920] {
            telar::set_theme(crate::theme::LandingTheme::light());
            let mut tree = telar::ComponentList::new(crate::app::LandingRoot.root());
            tree.on_event(&Event::WindowResized {
                width: w,
                height: 900,
            });
            let cmds = tree.commands();

            // (command_index, top, bottom, is_full_width_band)
            let mut items: Vec<(usize, f32, f32, bool)> = Vec::new();
            let mut ty = 0.0f32;
            let mut stack = vec![0.0f32];
            for (i, c) in cmds.iter().enumerate() {
                match c {
                    PushMatrix { matrix } => {
                        ty += matrix[5];
                        stack.push(ty);
                    }
                    PopMatrix => {
                        stack.pop();
                        ty = *stack.last().unwrap();
                    }
                    Rect { rect, .. } | Text { rect, .. } | Image { rect, .. } => {
                        let band = matches!(c, Rect { .. }) && rect.width >= w as f32 - 4.0;
                        items.push((i, ty + rect.y, ty + rect.y + rect.height, band));
                    }
                    _ => {}
                }
            }
            for &(bi, btop, bbot, is_band) in &items {
                if !is_band {
                    continue;
                }
                for &(ci, ctop, cbot, _) in &items {
                    if ci >= bi {
                        continue; // only content painted before this band can be covered
                    }
                    let overlap = cbot.min(bbot) - ctop.max(btop);
                    assert!(
                        overlap <= 2.0,
                        "content (cmd {ci}, y {ctop}..{cbot}) hidden behind band \
                         (cmd {bi}, y {btop}..{bbot}) at width {w}"
                    );
                }
            }
        }
    }

    // Full-width section bands must not overlap vertically at any width, including the ones where the feature cards wrap to a second line.
    #[test]
    fn section_bands_do_not_overlap() {
        use telar::{App, Event};
        for w in [820u32, 900, 1000, 1100, 1180] {
            telar::set_theme(crate::theme::LandingTheme::light());
            let mut tree = telar::ComponentList::new(crate::app::LandingRoot.root());
            tree.on_event(&Event::WindowResized {
                width: w,
                height: 800,
            });
            let mut bands = {
                let cmds = tree.commands();
                collect_bands(&cmds, w as f32 - 4.0)
            };
            bands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            for pair in bands.windows(2) {
                let (_, bottom) = pair[0];
                let (next_top, _) = pair[1];
                assert!(
                    next_top >= bottom - 1.0,
                    "bands overlap at width {w}: band ends {bottom}, next starts {next_top}"
                );
            }
        }
    }

    // The drawn content must actually widen with the window, not stay fixed.
    #[test]
    fn drawn_content_follows_width() {
        use telar::{App, Event};
        telar::set_theme(crate::theme::LandingTheme::light());
        let mut tree = telar::ComponentList::new(crate::app::LandingRoot.root());

        tree.on_event(&Event::WindowResized {
            width: 1400,
            height: 900,
        });
        let wide = content_right_edge(&tree.commands());

        tree.on_event(&Event::WindowResized {
            width: 600,
            height: 900,
        });
        let narrow = content_right_edge(&tree.commands());

        assert!(
            wide > narrow + 300.0,
            "content did not widen: wide={wide} narrow={narrow}"
        );
    }

    // Reproduces the live-app flow: feed the SAME root tree several WindowResized events and confirm every resize re-flattens to different draw commands. A dropped subscription would show up as an unchanged generation after the first.
    #[test]
    fn live_tree_relayouts_on_every_resize() {
        use telar::{App, Event};
        telar::set_theme(crate::theme::LandingTheme::light());
        let mut tree = telar::ComponentList::new(crate::app::LandingRoot.root());

        let mut last_gen = None;
        for (w, h) in [(1400, 900), (600, 900), (1000, 700), (500, 1000)] {
            tree.on_event(&Event::WindowResized {
                width: w,
                height: h,
            });
            let _ = tree.commands();
            let g = tree.generation();
            if let Some(prev) = last_gen {
                assert_ne!(prev, g, "resize to {w}x{h} did not change the output");
            }
            last_gen = Some(g);
        }
    }
}
