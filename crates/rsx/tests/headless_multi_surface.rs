//! Integration test for the multi-surface contract (Sprint C): drive N real rsx apps in one run through
//! `run_multi_with_platform` + `HeadlessPlatform`. Each surface runs on its own thread with its own reactive
//! tree, renders a distinct color, and its pixels are captured by SurfaceId — proving N isolated surfaces
//! render correctly in a single run with no cross-talk.

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::{FillApp, NullPaths, assert_center_rgb};
use platform_headless::{HeadlessPlatform, SurfaceFrameSink};
use rsx::{AppConfig, AppPathsProvider, Color, SurfaceId, WindowConfig, run_multi_with_platform};

#[test]
fn headless_multi_surface_renders_isolated_trees() {
    // Distinct color per surface; each surface gets its own size too, to catch any surface/config mix-up.
    let specs: [(SurfaceId, [u8; 3], u32, u32); 3] = [
        (SurfaceId(0), [200, 40, 40], 32, 24),
        (SurfaceId(1), [40, 200, 40], 48, 16),
        (SurfaceId(2), [40, 40, 200], 20, 40),
    ];

    let surfaces: Vec<(SurfaceId, AppConfig)> = specs
        .iter()
        .map(|(id, _, w, h)| {
            let window = WindowConfig {
                width: *w,
                height: *h,
                ..WindowConfig::default()
            };
            (*id, AppConfig::from(window))
        })
        .collect();

    // The color each surface's app paints, shared read-only across the surface threads.
    let colors: Arc<HashMap<SurfaceId, [u8; 3]>> =
        Arc::new(specs.iter().map(|(id, rgb, _, _)| (*id, *rgb)).collect());

    let results: SurfaceFrameSink = Arc::new(Mutex::new(HashMap::new()));
    let platform = HeadlessPlatform::new(1, 1)
        .with_frames(2)
        .capture_surfaces_into(results.clone());

    let colors_for_factory = Arc::clone(&colors);
    run_multi_with_platform(
        platform,
        surfaces,
        |_id| Box::new(NullPaths) as Box<dyn AppPathsProvider>,
        move |id| {
            let rgb = colors_for_factory[&id];
            FillApp {
                color: Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]),
            }
        },
        "rsx-headless-multi",
    )
    .expect("multi-surface run failed");

    let out = results.lock().unwrap();
    assert_eq!(out.len(), specs.len(), "every surface produced a frame");
    for (id, rgb, w, h) in specs.iter() {
        let pixels = out
            .get(id)
            .unwrap_or_else(|| panic!("surface {id:?} produced no frame"));
        assert_center_rgb(pixels, *w, *h, *rgb, &format!("surface {id:?}"));
    }
}
