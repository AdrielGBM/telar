use std::sync::Arc;

use geometry_core::{Point, Rect};
use renderer_core::{
    Border, BorderRadius, Color, Declared, DrawCommand, Element, ElementId, PathData, PathStyle,
    RectStyle, Role, Semantics, Shadow, ShapeStyle, Span, Stroke, TextStyle,
};

use super::*;

fn rect_command(x: f32, y: f32, style: RectStyle) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, y, 80.0, 32.0),
        style: Arc::new(style),
    }
}

#[test]
fn a_rect_names_only_what_it_sets() {
    let plain = rect_command(0.0, 0.0, RectStyle::default().with_fill(Color::WHITE));
    let dressed = rect_command(
        8.0,
        4.0,
        RectStyle::filled(Color::from_rgb_u8(0x1a, 0x2b, 0x3c), 6.0)
            .with_border(Border::per_side(Color::BLACK, 1.0, 0.0, 1.0, 0.0))
            .with_shadow(Shadow::new(0.0, 2.0, 4.0, Color::rgba(0.0, 0.0, 0.0, 0.25))),
    );
    assert_eq!(
        draw_text(&[plain, dressed]),
        "rect 0,0 80x32 fill=#ffffffff\n\
         rect 8,4 80x32 fill=#1a2b3cff border=1,0,1,0 border-color=#000000ff radius=6 shadow=0,2,4,0,#00000040\n"
    );
}

#[test]
fn geometry_is_rounded_to_hundredths_and_never_negative_zero() {
    let commands = [DrawCommand::Rect {
        rect: Rect::new(10.004, -0.001, 12.345_678, 1.0 / 3.0),
        style: Arc::new(RectStyle::default()),
    }];
    assert_eq!(draw_text(&commands), "rect 10,0 12.35x0.33\n");
}

#[test]
fn what_a_clip_or_an_element_encloses_is_indented_under_it() {
    let element = Element::new(
        ElementId(42),
        Semantics {
            label: Some("Save".into()),
            ..Semantics::of(Role::Button)
        },
        "",
        Rect::new(0.0, 0.0, 80.0, 32.0),
    );
    let commands = [
        DrawCommand::PushElement {
            element: Arc::new(element),
        },
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 80.0, 32.0),
            radius: BorderRadius::all(4.0),
        },
        rect_command(0.0, 0.0, RectStyle::default()),
        DrawCommand::PopClip,
        DrawCommand::PopElement,
    ];
    assert_eq!(
        draw_text(&commands),
        "element button 0,0 80x32 label=\"Save\"\n\
         \x20 clip 0,0 80x32 radius=4\n\
         \x20   rect 0,0 80x32\n\
         \x20 end\n\
         end\n",
        "and no element id, which changes from run to run"
    );
}

#[test]
fn text_names_its_face_size_and_ink_and_a_span_only_what_it_changes() {
    let commands = [DrawCommand::Text {
        text: Arc::from("Hello \"you\""),
        spans: Some(Arc::from([Span::new(
            6..11,
            Declared {
                font_weight: Some(700),
                ..Declared::default()
            },
        )])),
        rect: Rect::new(16.0, 12.0, 120.0, 20.0),
        style: Arc::new(
            TextStyle::new(14.0, Color::BLACK)
                .with_font_family("Telar Test")
                .with_letter_spacing(0.5),
        ),
    }];
    assert_eq!(
        draw_text(&commands),
        "text 16,12 120x20 \"Hello \\\"you\\\"\" size=14 family=\"Telar Test\" weight=400 color=#000000ff letter-spacing=0.5 span=6..11[weight=700]\n"
    );
}

#[test]
fn a_path_writes_its_verbs_and_a_line_its_stroke() {
    let data = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(10.0, 0.0))
        .close();
    let commands = [
        DrawCommand::Path {
            data: Arc::new(data),
            style: Arc::new(PathStyle::default().with_fill(Color::RED)),
        },
        DrawCommand::Line {
            p1: Point::new(0.0, 0.5),
            p2: Point::new(10.0, 0.5),
            style: Arc::new(Stroke::new(Color::BLUE, 1.0).with_dash(&[4.0, 2.0], 0.0)),
        },
    ];
    assert_eq!(
        draw_text(&commands),
        "path M0,0 L10,0 Z fill=#ff0000ff\n\
         line 0,0.5 10,0.5 stroke=1 stroke-color=#0000ffff dash=4,2@0\n"
    );
}

#[test]
fn a_recorded_frame_opens_with_its_size_and_clear_colour() {
    let frame = RecordedFrame {
        width: 320,
        height: 240,
        scale_factor: 2.0,
        generation: 3,
        clear: Some(Color::WHITE),
        commands: vec![rect_command(0.0, 0.0, RectStyle::default())],
    };
    assert_eq!(
        frame.to_text(),
        "frame 320x240 clear=#ffffffff\nrect 0,0 80x32\n"
    );
}

#[test]
fn an_element_laid_out_by_its_parent_names_only_its_role() {
    let element = Element::new(
        ElementId(7),
        Semantics::group(),
        "",
        Rect::new(0.0, 0.0, 0.0, 0.0),
    );
    let commands = [
        DrawCommand::PushElement {
            element: Arc::new(element),
        },
        DrawCommand::PopElement,
    ];
    assert_eq!(draw_text(&commands), "element group\nend\n");
}
