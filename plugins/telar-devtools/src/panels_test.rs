use std::rc::Rc;
use std::sync::Arc;

use telar::{
    AccessAudit, AccessNode, AccessRule, AccessViolation, DrawCommand, Insets, LayoutItem, Rect,
    Role, SegmentNodeInfo, Size, SurfaceCanvas, Transform, box_item, for_each_with_matrix,
    transform_clip_rect,
};

use super::*;
use crate::inspect::InspectSource;
use crate::workbench::workbench_scope;

const SIZE: Size = Size::new(600.0, 400.0);

/// A source whose walk and audit are whatever the test says.
struct Given {
    nodes: Vec<SegmentNodeInfo>,
    audit: Option<AccessAudit>,
    placement: Transform,
}

impl InspectSource for Given {
    fn generation(&self) -> u64 {
        0
    }

    fn walk(&self) -> Vec<SegmentNodeInfo> {
        self.nodes.clone()
    }

    fn audit(&self) -> Option<AccessAudit> {
        self.audit.clone()
    }

    fn placement(&self) -> Transform {
        self.placement
    }
}

fn node(id: u64, name: &'static str, depth: usize, rect: Rect) -> SegmentNodeInfo {
    SegmentNodeInfo {
        id,
        name,
        depth,
        rect,
        padding: Insets::all(4.0),
        margin: Insets::all(2.0),
        border: Insets::all(1.0),
        gap: Size::ZERO,
    }
}

fn walk() -> Vec<SegmentNodeInfo> {
    vec![
        node(0, "Page", 0, Rect::new(0.0, 0.0, 300.0, 200.0)),
        node(1, "SaveButton", 1, Rect::new(20.0, 30.0, 100.0, 40.0)),
    ]
}

fn access_node(name: &str, role: Role, rect: Rect) -> AccessNode {
    AccessNode {
        id: Some(1),
        parent: None,
        role,
        name: name.to_owned(),
        rect,
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        lang: None,
        url: None,
        current: None,
        orientation: None,
        expanded: None,
        position: None,
        active_descendant: None,
        controls: None,
        has_popup: None,
        keyboard_shortcut: None,
        sort: None,
        multiselectable: false,
        modal: false,
    }
}

fn audit() -> AccessAudit {
    AccessAudit {
        nodes: vec![access_node(
            "Save",
            Role::Button,
            Rect::new(20.0, 30.0, 100.0, 40.0),
        )],
        violations: vec![AccessViolation::new(
            AccessRule::Contrast,
            "\"Save\" is 2.10:1 against its background",
            Some(Rect::new(30.0, 40.0, 60.0, 16.0)),
        )],
    }
}

/// `build` on a canvas of its own in the workbench theme, laid out at [`SIZE`].
fn mounted(
    build: impl FnOnce() -> Result<Box<dyn LayoutItem>, telar::LayoutError> + 'static,
) -> SurfaceCanvas {
    telar::install_default_text_metrics();
    let canvas = SurfaceCanvas::new(SIZE, || Ok(box_item(workbench_scope(build)?))).unwrap();
    canvas.resize(SIZE);
    canvas
}

fn frame(canvas: &SurfaceCanvas) -> Vec<DrawCommand> {
    canvas.relayout_if_dirty();
    canvas.frame_commands().clone()
}

fn texts(canvas: &SurfaceCanvas) -> Vec<String> {
    frame(canvas)
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

/// Every rect the frame fills or strokes, where it lands; a box that paints nothing is left out.
fn rects(canvas: &SurfaceCanvas) -> Vec<Rect> {
    let mut found = Vec::new();
    for_each_with_matrix(&frame(canvas), |command, matrix| {
        if let DrawCommand::Rect { rect, style } = command
            && (style.fill.is_some() || style.border.is_some())
        {
            found.push(transform_clip_rect(matrix, *rect));
        }
    });
    found
}

fn text_rect(drawn: &[DrawCommand], wanted: &str) -> Rect {
    drawn
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { text, rect, .. } if &**text == wanted => Some(*rect),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{wanted:?} is drawn"))
}

fn refreshed(inspection: Inspection, source: &Given) {
    inspection.refresh(source);
}

#[test]
fn the_accessibility_panel_lists_what_the_checks_found_over_what_a_reader_is_told() {
    let inspection = Rc::new(std::cell::Cell::new(None));
    let kept = Rc::clone(&inspection);
    let canvas = mounted(move || {
        let made = Inspection::new();
        kept.set(Some(made));
        accessibility_panel(made)
    });
    let inspection = inspection.get().expect("the panel was built");
    assert!(
        inspection.wants_audit(),
        "a mounted panel asks for the audit"
    );
    assert!(
        texts(&canvas)
            .iter()
            .any(|text| text.starts_with("Nothing to show")),
        "with nothing read yet the panel says so: {:?}",
        texts(&canvas)
    );

    let source = Given {
        nodes: walk(),
        audit: Some(audit()),
        placement: Transform::IDENTITY,
    };
    {
        let _entered = canvas.enter();
        refreshed(inspection, &source);
    }
    let shown = texts(&canvas);
    for expected in [
        "1 errors · 0 warnings",
        "Problems",
        "\"Save\" is 2.10:1 against its background",
        "error · contrast",
        "What a reader is told",
        "Save",
        "button",
    ] {
        assert!(
            shown.iter().any(|text| text == expected),
            "{expected:?} is shown: {shown:?}"
        );
    }
}

#[test]
fn a_clean_audit_says_there_is_nothing_to_fix() {
    let inspection = Inspection::new();
    let shown = {
        let canvas = mounted(move || accessibility_panel(inspection));
        {
            let _entered = canvas.enter();
            refreshed(
                inspection,
                &Given {
                    nodes: Vec::new(),
                    audit: Some(AccessAudit::default()),
                    placement: Transform::IDENTITY,
                },
            );
        }
        texts(&canvas)
    };
    assert!(
        shown.iter().any(|text| text == "No problems found."),
        "{shown:?}"
    );
}

/// The summary going from a wrapped sentence to one line hands the rows below it more room in the same pass, and the pane closing right after must take the rows' scroll with it.
#[test]
fn the_summary_wraps_in_its_pane_and_the_pane_closes_cleanly_once_the_audit_is_in() {
    const PANE: f32 = 320.0;
    let shown = telar::signal(true);
    let inspection = Inspection::new();
    let canvas = mounted(move || {
        let layer = telar::ReactiveList::with_style(
            telar::LayoutStyle::new().absolute_fill().flex_row(),
            move || if shown.get() { vec![()] } else { Vec::new() },
            |_: &()| (),
            move |()| {
                telar_components::split_pane(
                    telar_components::SplitPaneProps::props()
                        .size(telar::signal(PANE))
                        .collapsible(false)
                        .build(),
                    panes([
                        accessibility_panel(inspection)?,
                        box_item(telar::Container::new(
                            telar::LayoutStyle::new().flex_grow(1.0),
                            Vec::new(),
                        )?),
                    ]),
                )
            },
        )?;
        Ok(box_item(layer))
    });
    let waiting = text_rect(
        &frame(&canvas),
        "Nothing to show: the app has not said what a reader is told.",
    );
    assert!(
        waiting.x + waiting.width <= PANE,
        "the sentence stays inside its pane: {waiting:?}"
    );

    {
        let _entered = canvas.enter();
        refreshed(
            inspection,
            &Given {
                nodes: Vec::new(),
                audit: Some(audit()),
                placement: Transform::IDENTITY,
            },
        );
    }
    // A second frame here would settle the rows and hide the failure: the unmount has to follow the very pass that grew their viewport.
    let counts = text_rect(&frame(&canvas), "1 errors · 0 warnings");
    assert!(
        waiting.height > counts.height,
        "it wraps onto more lines to do it, where the counts fit on one: {waiting:?} against {counts:?}"
    );

    {
        let _entered = canvas.enter();
        shown.set(false);
    }
    assert!(
        !texts(&canvas).iter().any(|text| text == "Problems"),
        "the pane is gone"
    );
    assert!(
        !inspection.wants_audit(),
        "and with it the audit it asked for"
    );
}

#[test]
fn the_selected_component_is_marked_with_its_box_model_where_the_source_is_shown() {
    let inspection = Inspection::new();
    let canvas = mounted(move || {
        Ok(box_item(telar::Container::new(
            telar::LayoutStyle::new()
                .width(telar::SizeDimension::Percent(1.0))
                .height(telar::SizeDimension::Percent(1.0)),
            vec![inspector_panel(inspection)?, inspection_marks(inspection)?],
        )?))
    });
    let source = Given {
        nodes: walk(),
        audit: None,
        placement: Transform::translate(50.0, 60.0),
    };
    {
        let _entered = canvas.enter();
        refreshed(inspection, &source);
        inspection.selected.set(Some(Arc::from("1")));
    }
    let drawn = rects(&canvas);
    let shown = |rect: Rect| Rect::new(rect.x + 50.0, rect.y + 60.0, rect.width, rect.height);
    assert!(
        drawn.contains(&shown(Rect::new(20.0, 30.0, 100.0, 40.0))),
        "the border box, moved where the source is shown"
    );
    assert!(
        drawn.contains(&shown(Rect::new(25.0, 35.0, 90.0, 30.0))),
        "the content box, inside the border and the padding"
    );
    assert!(
        drawn.contains(&shown(Rect::new(18.0, 28.0, 104.0, 2.0))),
        "the margin band above it"
    );
    assert!(
        texts(&canvas).iter().any(|text| text == "100 × 40"),
        "and its details"
    );
}

#[test]
fn the_outline_and_the_measure_follow_their_toggles() {
    let inspection = Inspection::new();
    let canvas = mounted(move || inspection_marks(inspection));
    {
        let _entered = canvas.enter();
        refreshed(
            inspection,
            &Given {
                nodes: walk(),
                audit: None,
                placement: Transform::IDENTITY,
            },
        );
        inspection.selected.set(Some(Arc::from("1")));
    }
    assert!(rects(&canvas).is_empty(), "nothing is marked unasked");

    inspection.outline().set(true);
    let outlined = rects(&canvas);
    for rect in [
        Rect::new(0.0, 0.0, 300.0, 200.0),
        Rect::new(20.0, 30.0, 100.0, 40.0),
    ] {
        assert!(
            outlined.contains(&rect),
            "{rect:?} is outlined: {outlined:?}"
        );
    }

    inspection.outline().set(false);
    inspection.measure().set(true);
    let measured = texts(&canvas);
    for distance in ["100 × 40", "20", "30", "180", "130"] {
        assert!(
            measured.iter().any(|text| text == distance),
            "{distance} is measured: {measured:?}"
        );
    }
}

#[test]
fn the_row_chosen_in_the_accessibility_panel_is_marked() {
    let inspection = Inspection::new();
    let canvas = mounted(move || {
        Ok(box_item(telar::Container::new(
            telar::LayoutStyle::new()
                .width(telar::SizeDimension::Percent(1.0))
                .height(telar::SizeDimension::Percent(1.0)),
            vec![
                accessibility_panel(inspection)?,
                inspection_marks(inspection)?,
            ],
        )?))
    });
    {
        let _entered = canvas.enter();
        refreshed(
            inspection,
            &Given {
                nodes: Vec::new(),
                audit: Some(audit()),
                placement: Transform::IDENTITY,
            },
        );
        inspection
            .access_selected
            .set(Some(Arc::from(format!("{VIOLATIONS_ROW}/0").as_str())));
    }
    assert!(rects(&canvas).contains(&Rect::new(30.0, 40.0, 60.0, 16.0)));
}

#[test]
fn a_ring_is_what_the_outer_box_covers_outside_the_inner() {
    let strips = marks::ring(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Rect::new(2.0, 1.0, 5.0, 6.0),
    );
    assert_eq!(
        strips,
        [
            Rect::new(0.0, 0.0, 10.0, 1.0),
            Rect::new(0.0, 7.0, 10.0, 3.0),
            Rect::new(0.0, 1.0, 2.0, 6.0),
            Rect::new(7.0, 1.0, 3.0, 6.0),
        ]
    );
}

/// The reader's rows nest as the reader nests the nodes, each under the row of the node it sits in, and keep naming the node by its place in the audit.
#[test]
fn the_reader_rows_nest_as_a_reader_steps_through_them() {
    let at = Rect::new(0.0, 0.0, 10.0, 10.0);
    let inside = |parent: usize, node: AccessNode| AccessNode {
        parent: Some(parent),
        ..node
    };
    let audit = AccessAudit {
        nodes: vec![
            access_node("Files", Role::Grid, at),
            inside(0, access_node("", Role::Row, at)),
            inside(1, access_node("Name", Role::ColumnHeader, at)),
            access_node("Save", Role::Button, at),
        ],
        violations: Vec::new(),
    };
    let rows = accessibility::audit_rows(&audit);
    let reader = rows
        .iter()
        .find(|row| &*row.id == READER_ROW)
        .expect("the reader's branch");
    let ids = |rows: &[telar_components::TreeNode]| -> Vec<String> {
        rows.iter().map(|row| row.id.to_string()).collect()
    };
    assert_eq!(ids(&reader.children), ["reader/0", "reader/3"]);
    assert_eq!(ids(&reader.children[0].children), ["reader/1"]);
    assert_eq!(ids(&reader.children[0].children[0].children), ["reader/2"]);
}
