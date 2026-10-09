use telar::preview::{Layout, PreviewCtx, PreviewEntry};
use telar::testing::{centre, mount, press, release, route, texts};
use telar::{
    AccessNode, App, AppRuntime, Color, ComponentList, Container, DrawCommand, LayoutError,
    LayoutItem, LayoutStyle, LocalApp, Rect, Role, Text, TextStyle, box_item, effect,
    for_each_with_matrix, high_contrast_override, reduced_motion_override,
    set_high_contrast_override, set_reduced_motion_override, transform_clip_rect, use_direction,
    use_high_contrast, use_locale, use_safe_area_insets,
};

use crate::WorkshopApp;
use crate::test_support::{control_matching, drawn, settle};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1000;
const PROJECT: &str = "Studio";
const NIGHT: Color = Color::rgb(0.05, 0.1, 0.3);

#[derive(Clone)]
struct Night;

impl theme_core::ThemeTokens for Night {
    fn surface(&self) -> Color {
        NIGHT
    }
}

fn line(text: impl Fn() -> String + 'static) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Text::new(text, LayoutStyle::new(), || {
        TextStyle::new(13.0, Color::BLACK)
    })?))
}

/// What the canvas reads, in a row that starts at whichever edge its direction starts rows at.
fn environment(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let read = line(|| {
        format!(
            "env {:?} {}",
            use_direction(),
            use_locale().unwrap_or_else(|| "none".into())
        )
    })?;
    let row = Container::new(
        LayoutStyle::new().flex_row().gap(8.0),
        vec![read, line(|| "tail".into())?],
    )?;
    Ok(box_item(row))
}

fn safe_area(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    line(|| {
        let insets = use_safe_area_insets();
        format!(
            "safe {} {} {} {}",
            insets.top, insets.right, insets.bottom, insets.left
        )
    })
}

/// The safe area as the build finds it, read once and never again.
fn safe_area_at_build(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let insets = use_safe_area_insets();
    line(move || {
        format!(
            "built in {} {} {} {}",
            insets.top, insets.right, insets.bottom, insets.left
        )
    })
}

fn contrast(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    line(|| format!("contrast {:?}", use_high_contrast()))
}

/// The previews the toolbar is tried on. The second only lists its locale, which the toolbar then offers.
fn entries(
    build: fn(&PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new("fake--probe--env", "probe", "Env", build),
        PreviewEntry::new("fake--probe--arabic", "probe", "Arabic", build).locale("ar"),
    ]
}

struct Workshop {
    runtime: LocalApp<WorkshopApp>,
    tree: ComponentList,
}

impl Workshop {
    fn open(entries: Vec<PreviewEntry>) -> Self {
        telar::install_default_text_metrics();
        let runtime = LocalApp(WorkshopApp::new(entries).project_name(PROJECT));
        let tree = mount(runtime.0.root(), WIDTH, HEIGHT);
        let workshop = Self { runtime, tree };
        settle(&workshop.tree);
        workshop
    }

    fn draws(&self, text: &str) -> bool {
        texts(&self.tree).iter().any(|drawn| drawn == text)
    }

    fn control(&self, role: Role, name: &str) -> AccessNode {
        let nodes = self.runtime.access_snapshot(&self.tree.commands());
        control_matching(&nodes, role, name, |node| named(node, name))
    }

    fn click(&mut self, role: Role, name: &str) {
        let rect = self.control(role, name).rect;
        let (x, y) = centre(rect);
        for event in [press(x, y), release(x, y)] {
            route(&mut self.tree, &event);
            settle(&self.tree);
        }
    }

    fn pick(&mut self, select: &str, row: &str) {
        self.click(Role::ComboBox, select);
        self.click(Role::MenuItem, row);
    }

    fn rect_of(&self, needle: &str) -> Rect {
        drawn(&self.tree)
            .into_iter()
            .find(|drawn| drawn.text == needle)
            .map(|drawn| drawn.rect)
            .unwrap_or_else(|| panic!("{needle} is not drawn"))
    }

    /// The canvas's rect: the clip `needle`, drawn inside it, is drawn under.
    fn canvas_rect(&self, needle: &str) -> Rect {
        drawn(&self.tree)
            .into_iter()
            .find(|drawn| drawn.text.starts_with(needle))
            .and_then(|drawn| drawn.clip)
            .unwrap_or_else(|| panic!("{needle} is not drawn inside a canvas"))
    }

    /// Each rect filled, with its colour, and whether it lies inside `canvas`.
    fn fills(&self, canvas: Rect) -> Vec<(Rect, Color, bool)> {
        let mut found = Vec::new();
        for_each_with_matrix(&self.tree.commands(), |command, matrix| {
            if let DrawCommand::Rect { rect, style } = command
                && let Some(telar::Paint::Solid(fill)) = &style.fill
            {
                let rect = transform_clip_rect(matrix, *rect);
                found.push((rect, *fill, inside(rect, canvas)));
            }
        });
        found
    }
}

/// Whether `node` is called `name`: a select is announced by its name and then what it shows.
fn named(node: &AccessNode, name: &str) -> bool {
    node.name == name || node.name.starts_with(&format!("{name} "))
}

fn inside(inner: Rect, outer: Rect) -> bool {
    const SLACK: f32 = 0.5;
    inner.x >= outer.x - SLACK
        && inner.y >= outer.y - SLACK
        && inner.x + inner.width <= outer.x + outer.width + SLACK
        && inner.y + inner.height <= outer.y + outer.height + SLACK
}

#[test]
fn mode_locale_and_direction_restyle_the_canvas_and_never_the_chrome() {
    theme_core::register_mode_theme("night", Night);
    let mut workshop = Workshop::open(entries(environment));
    assert!(workshop.draws("env Ltr none"));
    let canvas = workshop.canvas_rect("env");
    let chrome_texts = [PROJECT, "Env", "Arabic", "Controls"];
    let texts_before: Vec<Rect> = chrome_texts
        .iter()
        .map(|text| workshop.rect_of(text))
        .collect();
    let chrome_fills = |workshop: &Workshop| -> Vec<(Rect, Color)> {
        workshop
            .fills(canvas)
            .into_iter()
            .filter(|(.., in_canvas)| !in_canvas)
            .map(|(rect, color, _)| (rect, color))
            .collect()
    };
    let fills_before = chrome_fills(&workshop);
    assert!(
        !workshop
            .fills(canvas)
            .iter()
            .any(|(_, color, _)| *color == NIGHT),
        "the canvas starts in the application's theme"
    );

    workshop.pick("Mode", "night");
    workshop.pick("Locale", "ar");

    assert!(
        workshop.draws("env Rtl ar"),
        "the canvas reads in Arabic, right to left: {:?}",
        texts(&workshop.tree)
    );
    let night: Vec<_> = workshop
        .fills(canvas)
        .into_iter()
        .filter(|(_, color, _)| *color == NIGHT)
        .collect();
    assert!(!night.is_empty(), "the canvas takes the night theme");
    assert!(
        night.iter().all(|(.., in_canvas)| *in_canvas),
        "only inside the canvas: {night:?}"
    );
    let (probe, tail) = (workshop.rect_of("env Rtl ar"), workshop.rect_of("tail"));
    assert!(
        tail.x < probe.x,
        "the row inside the canvas starts at its right edge: {tail:?} before {probe:?}"
    );

    let texts_after: Vec<Rect> = chrome_texts
        .iter()
        .map(|text| workshop.rect_of(text))
        .collect();
    assert_eq!(
        texts_after, texts_before,
        "the chrome reads left to right where it did"
    );
    assert_eq!(
        chrome_fills(&workshop),
        fills_before,
        "the chrome keeps its colours and its layout"
    );
}

#[test]
fn the_preview_locale_is_the_default_and_a_chosen_direction_wins_over_the_locale() {
    let mut workshop = Workshop::open(vec![
        PreviewEntry::new("fake--probe--arabic", "probe", "Arabic", environment).locale("ar"),
        PreviewEntry::new("fake--probe--english", "probe", "English", environment).locale("en"),
    ]);
    assert!(workshop.draws("env Rtl ar"), "the preview's own locale");
    workshop.pick("Locale", "en");
    assert!(workshop.draws("env Ltr en"), "the toolbar's, over it");
    workshop.pick("Direction", "Right to left");
    assert!(workshop.draws("env Rtl en"), "a chosen direction wins");
    workshop.pick("Direction", "Auto direction");
    workshop.pick("Locale", "Default locale");
    assert!(workshop.draws("env Rtl ar"), "and back to the preview's");
}

#[test]
fn a_device_frame_sets_the_safe_area_and_rotating_turns_it() {
    let mut workshop = Workshop::open(entries(safe_area));
    assert!(workshop.draws("safe 0 0 0 0"));
    workshop.pick("Viewport", "Phone");
    assert!(workshop.draws("safe 47 0 34 0"));
    assert!(workshop.draws("390 × 844"), "{:?}", texts(&workshop.tree));

    workshop.click(Role::Button, "Rotate");
    assert!(workshop.draws("safe 0 34 0 47"));
    assert!(workshop.draws("844 × 390"));

    workshop.pick("Viewport", "Fill");
    assert!(
        workshop.draws("safe 0 0 0 0"),
        "only a device keeps a safe area"
    );
}

#[test]
fn a_preview_mounted_in_a_device_frame_is_built_in_its_safe_area() {
    let mut workshop = Workshop::open(entries(safe_area_at_build));
    assert!(workshop.draws("built in 0 0 0 0"));
    workshop.pick("Viewport", "Phone");
    workshop.click(Role::Button, "Remount");
    assert!(
        workshop.draws("built in 47 0 34 0"),
        "{:?}",
        texts(&workshop.tree)
    );
}

#[test]
fn a_viewport_larger_than_the_stage_is_zoomed_to_fit_until_a_zoom_is_chosen() {
    let mut workshop = Workshop::open(vec![
        PreviewEntry::new("fake--probe--wide", "probe", "Wide", environment)
            .viewport(4000.0, 3000.0),
    ]);
    let fitted = workshop.canvas_rect("env");
    assert!(
        fitted.width < WIDTH as f32 && fitted.height < HEIGHT as f32,
        "the whole canvas shows: {fitted:?}"
    );
    assert!((fitted.width / fitted.height - 4.0 / 3.0).abs() < 0.01);
    assert!(workshop.draws("4000 × 3000"), "at the size it asked for");

    workshop.pick("Zoom", "50%");
    let halved = workshop.canvas_rect("env");
    assert_eq!((halved.width, halved.height), (2000.0, 1500.0));
    assert!(workshop.draws("4000 × 3000"));
}

#[test]
fn reduced_motion_is_switched_for_the_whole_application() {
    set_reduced_motion_override(None);
    let mut workshop = Workshop::open(entries(environment));
    assert_eq!(reduced_motion_override(), None);
    workshop.click(Role::Button, "Reduce motion");
    assert_eq!(reduced_motion_override(), Some(true));
    workshop.click(Role::Button, "Reduce motion");
    assert_eq!(reduced_motion_override(), None);
}

#[test]
fn high_contrast_is_switched_for_the_canvas_and_never_the_chrome() {
    set_high_contrast_override(None);
    let chrome = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let _chrome = {
        let chrome = chrome.clone();
        effect(move || chrome.borrow_mut().push(use_high_contrast()))
    };
    let mut workshop = Workshop::open(entries(contrast));
    assert!(workshop.draws("contrast None"));

    workshop.click(Role::Button, "High contrast");
    assert!(
        workshop.draws("contrast Some(true)"),
        "the canvas is shown in high contrast: {:?}",
        texts(&workshop.tree)
    );
    assert_eq!(
        high_contrast_override(),
        None,
        "the application keeps its own"
    );
    assert_eq!(*chrome.borrow(), vec![None], "the chrome never hears of it");

    workshop.click(Role::Button, "High contrast");
    assert!(
        workshop.draws("contrast None"),
        "released, the canvas follows the application again"
    );
    assert_eq!(*chrome.borrow(), vec![None]);
}

#[test]
fn a_fullscreen_preview_offers_no_size_zoom_or_rotation() {
    let workshop = Workshop::open(vec![
        PreviewEntry::new("fake--probe--full", "probe", "Full", environment)
            .layout(Layout::Fullscreen),
    ]);
    let nodes = workshop.runtime.access_snapshot(&workshop.tree.commands());
    for name in ["Viewport", "Zoom", "Rotate"] {
        assert!(
            !nodes.iter().any(|node| named(node, name)),
            "{name} is offered"
        );
    }
    assert!(nodes.iter().any(|node| named(node, "Mode")));
}

#[test]
fn reduced_motion_is_followed_while_the_workshop_shows_docs() {
    set_reduced_motion_override(None);
    telar::hot_restore_json(r#"{"@workshop/view":"\"Docs\""}"#);
    let mut workshop = Workshop::open(entries(environment));
    workshop.click(Role::Button, "Reduce motion");
    assert_eq!(
        reduced_motion_override(),
        Some(true),
        "no canvas is mounted, and motion is still switched"
    );
    workshop.click(Role::Button, "Reduce motion");
    assert_eq!(reduced_motion_override(), None);
}

/// The rects at most a hairline thick drawn inside `canvas`: the grid's lines.
fn hairlines(workshop: &Workshop, canvas: Rect) -> usize {
    workshop
        .fills(canvas)
        .into_iter()
        .filter(|(rect, _, inside)| *inside && (rect.width <= 1.0 || rect.height <= 1.0))
        .count()
}

#[test]
fn the_grid_lines_the_canvas_every_eight_of_its_px() {
    let mut workshop = Workshop::open(vec![
        PreviewEntry::new("fake--probe--sized", "probe", "Sized", environment)
            .viewport(400.0, 300.0),
    ]);
    let canvas = workshop.canvas_rect("env");
    let before = hairlines(&workshop, canvas);
    workshop.click(Role::Button, "Grid");
    assert_eq!(workshop.control(Role::Button, "Grid").toggled, Some(true));
    assert_eq!(
        hairlines(&workshop, canvas) - before,
        49 + 37,
        "a line every 8 px across 400 and down 300, the edges left to the frame"
    );
    workshop.click(Role::Button, "Grid");
    assert_eq!(hairlines(&workshop, canvas), before);
}

#[test]
fn the_rulers_measure_the_canvas_from_its_origin() {
    let mut workshop = Workshop::open(vec![
        PreviewEntry::new("fake--probe--sized", "probe", "Sized", environment)
            .viewport(400.0, 300.0),
    ]);
    assert!(!workshop.draws("100"));
    workshop.click(Role::Button, "Rulers");
    for label in ["0", "100", "200", "300"] {
        assert!(
            workshop.draws(label),
            "{label}: {:?}",
            texts(&workshop.tree)
        );
    }
    let canvas = workshop.canvas_rect("env");
    let zero = drawn(&workshop.tree)
        .into_iter()
        .filter(|drawn| drawn.text == "100")
        .map(|drawn| drawn.rect)
        .find(|rect| rect.y < canvas.y)
        .expect("the top ruler labels 100");
    assert!(
        (zero.x - (canvas.x + 100.0)).abs() <= 3.0,
        "100 is labelled 100 px into the canvas: {zero:?} against {canvas:?}"
    );
}

#[test]
fn a_fullscreen_preview_offers_the_grid_but_no_rulers() {
    let workshop = Workshop::open(vec![
        PreviewEntry::new("fake--probe--full", "probe", "Full", environment)
            .layout(Layout::Fullscreen),
    ]);
    let nodes = workshop.runtime.access_snapshot(&workshop.tree.commands());
    assert!(nodes.iter().any(|node| named(node, "Grid")));
    assert!(!nodes.iter().any(|node| named(node, "Rulers")));
}
