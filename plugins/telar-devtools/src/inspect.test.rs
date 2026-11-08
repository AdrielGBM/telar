use std::cell::Cell;
use std::rc::Rc;

use telar::{
    AccessRule, Color, Container, LayoutStyle, RectStyle, Role, ShapeStyle, SizeDimension,
    StyledContainer, box_item, relayout_if_dirty,
};

use super::*;

fn info(id: u64, name: &'static str, depth: usize, rect: Rect) -> SegmentNodeInfo {
    SegmentNodeInfo {
        id,
        name: name.into(),
        depth,
        rect,
        padding: Insets::default(),
        margin: Insets::default(),
        border: Insets::default(),
        gap: Size::ZERO,
    }
}

/// A source that hands out a fixed walk and counts how often it is asked.
#[derive(Default)]
struct Counted {
    generation: Cell<u64>,
    walks: Cell<u32>,
    audits: Cell<u32>,
    placement: Cell<Option<Transform>>,
}

impl InspectSource for Counted {
    fn generation(&self) -> u64 {
        self.generation.get()
    }

    fn walk(&self) -> Vec<SegmentNodeInfo> {
        self.walks.set(self.walks.get() + 1);
        vec![
            info(0, "Root", 0, Rect::new(0.0, 0.0, 100.0, 100.0)),
            info(1, "Leaf", 1, Rect::new(10.0, 10.0, 20.0, 20.0)),
        ]
    }

    fn audit(&self) -> Option<AccessAudit> {
        self.audits.set(self.audits.get() + 1);
        Some(AccessAudit::default())
    }

    fn placement(&self) -> Transform {
        self.placement.get().unwrap_or(Transform::IDENTITY)
    }
}

#[test]
fn a_frame_already_read_is_not_walked_again() {
    let inspection = Inspection::new();
    let source = Counted::default();
    inspection.refresh(&source);
    inspection.refresh(&source);
    assert_eq!(source.walks.get(), 1);
    assert_eq!(inspection.nodes().len(), 2);

    source.generation.set(1);
    inspection.refresh(&source);
    assert_eq!(source.walks.get(), 2, "a new frame is read");

    source.placement.set(Some(Transform::translate(5.0, 6.0)));
    inspection.refresh(&source);
    assert_eq!(
        source.walks.get(),
        2,
        "a frame that only moved is not walked"
    );
    assert_eq!(inspection.placement.peek(), Transform::translate(5.0, 6.0));
}

#[test]
fn the_audit_is_read_only_while_a_panel_shows_it() {
    let inspection = Inspection::new();
    let source = Counted::default();
    inspection.refresh(&source);
    assert_eq!(source.audits.get(), 0, "nobody asked");
    assert!(inspection.audit().is_none());

    inspection.auditing.set(1);
    inspection.refresh(&source);
    assert_eq!(
        source.audits.get(),
        1,
        "a panel opened on a frame already walked"
    );
    assert!(inspection.audit().is_some());

    inspection.auditing.set(0);
    inspection.refresh(&source);
    assert!(
        inspection.audit().is_none(),
        "an audit nobody shows is dropped, so it is never shown stale"
    );
}

#[test]
fn a_branch_opens_the_first_time_it_is_seen_and_stays_as_left_after() {
    let inspection = Inspection::new();
    let source = Counted::default();
    inspection.refresh(&source);
    assert!(inspection.expanded.peek().contains("0"));

    inspection.expanded.set(Default::default());
    source.generation.set(1);
    inspection.refresh(&source);
    assert!(
        !inspection.expanded.peek().contains("0"),
        "a branch shut by hand stays shut"
    );
}

#[test]
fn the_window_source_reports_what_the_runner_last_handed_it() {
    let mut window = WindowSource::default();
    assert!(
        window.audit().is_none(),
        "nothing to show until the runner says"
    );
    let before = window.generation();
    window.set_nodes(&[info(0, "Root", 0, Rect::new(0.0, 0.0, 10.0, 10.0))]);
    window.set_violations(&[AccessViolation::new(
        AccessRule::UnnamedControl,
        "a button has no name",
        None,
    )]);
    assert!(window.generation() > before);
    assert_eq!(window.walk().len(), 1);
    assert_eq!(window.audit().map(|audit| audit.violations.len()), Some(1));
    assert_eq!(window.placement(), Transform::IDENTITY);
}

fn canvas() -> Rc<SurfaceCanvas> {
    telar::testing::install_test_face();
    let canvas = SurfaceCanvas::new(Size::new(200.0, 100.0), || {
        let unnamed = StyledContainer::new(
            LayoutStyle::new().width(40.0).height(30.0),
            |_| RectStyle::default().with_fill(Color::WHITE),
            Vec::new(),
        )?
        .control(Role::Button)
        .on_press(|| {});
        Ok(box_item(Container::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0))
                .padding_all(10.0),
            vec![box_item(unnamed)],
        )?))
    })
    .unwrap();
    canvas.resize(Size::new(200.0, 100.0));
    Rc::new(canvas)
}

#[test]
fn a_canvas_is_read_in_its_own_world_and_marked_where_its_frame_shows_it() {
    let canvas = canvas();
    canvas.set_placement(Transform::translate(30.0, 40.0));
    let inspection = Inspection::new();
    inspection.auditing.set(1);
    inspection.refresh(&canvas);

    let mut walked = Vec::new();
    SurfaceCanvas::walk(&canvas, &mut walked);
    assert_eq!(inspection.nodes().len(), walked.len());
    assert!(
        inspection
            .nodes()
            .iter()
            .any(|node| node.rect == Rect::new(10.0, 10.0, 40.0, 30.0)),
        "in the canvas's own coordinates"
    );
    assert_eq!(
        inspection.placement.peek(),
        Transform::translate(30.0, 40.0)
    );
    let audit = inspection.audit().expect("a canvas always has an audit");
    assert!(
        audit
            .violations
            .iter()
            .any(|violation| violation.rule == AccessRule::UnnamedControl),
        "{audit:#?}"
    );
}

#[test]
fn following_a_canvas_reads_it_after_every_layout_pass_and_starts_with_nothing_chosen() {
    let canvas = canvas();
    let inspection = Inspection::new();
    inspection.selected.set(Some(Arc::from("3")));
    let hook = inspection.follow(Rc::clone(&canvas));
    assert!(inspection.selected.peek().is_none());
    assert!(
        inspection.nodes().is_empty(),
        "nothing is read before a pass"
    );

    relayout_if_dirty();
    assert!(!inspection.nodes().is_empty());

    drop(hook);
    canvas.set_placement(Transform::translate(1.0, 1.0));
    relayout_if_dirty();
    assert_eq!(
        inspection.placement.peek(),
        Transform::IDENTITY,
        "a dropped hook reads nothing more"
    );
}

#[test]
fn the_outermost_component_is_chosen_while_nothing_is_or_what_was_is_gone() {
    let inspection = Inspection::new();
    let walk = |ids: &[u64]| -> Vec<SegmentNodeInfo> {
        ids.iter()
            .enumerate()
            .map(|(depth, id)| info(*id, "Box", depth, Rect::new(0.0, 0.0, 10.0, 10.0)))
            .collect()
    };
    inspection.take_walk(&walk(&[7, 8]));
    assert_eq!(inspection.selected.peek().as_deref(), Some("7"));

    inspection.selected.set(Some(Arc::from("8")));
    inspection.take_walk(&walk(&[7, 8, 9]));
    assert_eq!(
        inspection.selected.peek().as_deref(),
        Some("8"),
        "a choice still there is kept"
    );

    inspection.take_walk(&walk(&[3, 4]));
    assert_eq!(inspection.selected.peek().as_deref(), Some("3"));

    inspection.take_walk(&[]);
    assert_eq!(inspection.selected.peek(), None, "nothing to choose");
}
