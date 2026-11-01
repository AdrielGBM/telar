//! A surface's size reaching an application through the real runner: a headless window dragged to new sizes in, the size store, the layout of surface fractions and a followed breakpoint out.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use platform_headless::HeadlessPlatform;
use telar::{
    App, AppConfig, AppPathsProvider, Color, Component, Container, LayoutItem, LayoutStyle,
    NoPaths, RectStyle, Rectangle, Size, SizeDimension, WindowRoot, breakpoint, effect,
    reset_layout_runtime, run_with_platform, surface_size, track_layout,
};

#[derive(Default)]
struct Observed {
    size_at_build: RefCell<Option<Size>>,
    half_widths: RefCell<Vec<f32>>,
    layouts: RefCell<Vec<&'static str>>,
}

struct Responsive(Rc<Observed>);

impl App for Responsive {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        *self.0.size_at_build.borrow_mut() = Some(surface_size());

        let half = Rectangle::new(
            LayoutStyle::new()
                .width(SizeDimension::SurfaceWidth(0.5))
                .height(10.0),
            || RectStyle::filled(Color::from_rgb_u8(1, 2, 3), 0.0),
        )
        .unwrap();
        let node = half.layout_node();
        let observed = self.0.clone();
        let _ = effect(move || {
            let width = track_layout(node).map_or(0.0, |rect| rect.get().width);
            let mut widths = observed.half_widths.borrow_mut();
            if widths.last() != Some(&width) {
                widths.push(width);
            }
        });

        let layout = breakpoint("stacked").at(600.0, "side by side").follow();
        let observed = self.0.clone();
        let _ = effect(move || observed.layouts.borrow_mut().push(layout.get()));

        let page = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            vec![Box::new(half)],
        )
        .unwrap();
        Box::new(WindowRoot::new(Box::new(page)))
    }
}

fn run(platform: HeadlessPlatform) -> Rc<Observed> {
    let observed = Rc::new(Observed::default());
    run_with_platform::<_, _, ()>(
        platform,
        AppConfig::default(),
        Arc::new(NoPaths) as Arc<dyn AppPathsProvider>,
        Responsive(observed.clone()),
        "telar-surface-size-test",
    )
    .expect("headless run failed");
    observed
}

#[test]
fn the_tree_is_built_already_knowing_its_surface() {
    let observed = run(HeadlessPlatform::new(400, 300));
    assert_eq!(
        *observed.size_at_build.borrow(),
        Some(Size::new(400.0, 300.0)),
        "a tree built at zero would choose every breakpoint twice on its first frame"
    );
    assert_eq!(*observed.layouts.borrow(), ["stacked"]);
}

#[test]
fn a_resize_moves_surface_fractions_and_crosses_breakpoints_only_at_thresholds() {
    let observed =
        run(HeadlessPlatform::new(400, 300).with_resizes([(480, 300), (900, 300), (900, 120)]));
    assert_eq!(
        observed.half_widths.borrow().last(),
        Some(&450.0),
        "{:?}",
        observed.half_widths.borrow()
    );
    assert!(
        observed
            .half_widths
            .borrow()
            .ends_with(&[200.0, 240.0, 450.0]),
        "{:?}",
        observed.half_widths.borrow()
    );
    assert_eq!(
        *observed.layouts.borrow(),
        ["stacked", "side by side"],
        "480 is still narrow, and a height change is no crossing"
    );
}
