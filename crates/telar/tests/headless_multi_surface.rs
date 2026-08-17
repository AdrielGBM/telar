//! Integration test for the M3 multi-surface contract: drive N real rsx apps in one run through
//! `run_multi_with_platform` + `HeadlessPlatform`. All surfaces share **one** thread and one reactive
//! runtime, each with its own `Surface` world; each renders a distinct color at a distinct size and its
//! pixels are captured by SurfaceId — proving N surfaces render isolated on a single shared-runtime thread
//! with no cross-talk (T-8.1).

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::{FillApp, MaybePanicApp, assert_center_rgb};
use platform_headless::{HeadlessPlatform, SurfaceFrameSink};
use telar::NoPaths;
use telar::{AppConfig, AppPathsProvider, Color, SurfaceId, WindowConfig, run_multi_with_platform};

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
        |_id| std::sync::Arc::new(NoPaths) as std::sync::Arc<dyn AppPathsProvider>,
        move |id| {
            let rgb = colors_for_factory[&id];
            FillApp {
                color: Color::from_rgb_u8(rgb[0], rgb[1], rgb[2]),
            }
        },
        "telar-headless-multi",
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

// T-4.2 quarantine: when one surface's app panics during build, it must be unmounted while the other surface
// still renders — proving a panic in one surface on the shared UI thread does not tumble the rest. (Effective
// only under panic=unwind, as tests run; a panic=abort release build aborts instead of catching.)
#[test]
fn headless_multi_surface_quarantines_a_panicking_surface() {
    let good = SurfaceId(0);
    let bad = SurfaceId(1);
    let (w, h) = (24u32, 24u32);
    let make = |id: SurfaceId| {
        (
            id,
            AppConfig::from(WindowConfig {
                width: w,
                height: h,
                ..WindowConfig::default()
            }),
        )
    };

    let results: SurfaceFrameSink = Arc::new(Mutex::new(HashMap::new()));
    let platform = HeadlessPlatform::new(1, 1)
        .with_frames(2)
        .capture_surfaces_into(results.clone());

    run_multi_with_platform(
        platform,
        vec![make(good), make(bad)],
        |_id| std::sync::Arc::new(NoPaths) as std::sync::Arc<dyn AppPathsProvider>,
        move |id| MaybePanicApp {
            color: Color::from_rgb_u8(40, 200, 40),
            panic_on_build: id == bad,
        },
        "telar-headless-quarantine",
    )
    .expect("the run must complete even though one surface panicked");

    let out = results.lock().unwrap();
    assert!(
        out.contains_key(&good),
        "the healthy surface must still render"
    );
    assert!(
        !out.contains_key(&bad),
        "the panicking surface must be unmounted (no frame)"
    );
    assert_center_rgb(&out[&good], w, h, [40, 200, 40], "healthy surface");
}
