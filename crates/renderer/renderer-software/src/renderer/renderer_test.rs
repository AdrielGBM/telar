use platform_headless::HeadlessWindow;
use renderer_core::RenderBackend;

use super::SoftwareRenderer;
use crate::SoftwareRendererConfig;

// The caches are a thread-local, so which thread builds them decides whether the renderer sees its own fonts. Building them in the constructor furnished the UI thread for a renderer drawing on another. Run on a fresh thread, because the test binary's main thread may already have caches from another case.
#[test]
fn the_drawing_thread_builds_the_caches_not_the_constructing_one() {
    std::thread::spawn(|| {
        assert!(!crate::caches::initialised(), "a fresh thread starts empty");

        let mut renderer = SoftwareRenderer::<HeadlessWindow, HeadlessWindow>::new_headless(
            8,
            8,
            SoftwareRendererConfig::default(),
        );
        assert!(
            !crate::caches::initialised(),
            "constructing must not load fonts on this thread"
        );

        renderer.begin_frame(8, 8, 1.0, 0).unwrap();
        assert!(
            crate::caches::initialised(),
            "the first frame builds them, on the thread that draws"
        );
    })
    .join()
    .unwrap();
}
