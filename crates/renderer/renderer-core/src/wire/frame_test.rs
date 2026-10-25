use std::cell::Cell;
use std::sync::Arc;

use geometry_core::{Point, Rect};
use semantics_core::{
    ConsumedKeys, Destination, HasPopup, Location, Modal, NumericValue, Orientation, SetPosition,
    SortDirection, Uri,
};

use super::*;
use crate::{
    BlendMode, Border, BorderRadius, Color, Declared, Element, ElementId, ExternalTexture,
    FillRule, Focusable, FontFamily, FontFeatures, FontStyle, FontTag, FontVariations, Gradient,
    ImageData, ImageFill, ImageSlice, Insets, LayerMask, LineCap, LineJoin, Linked, Paint,
    PathData, PathStyle, Picture, Raster, RectStyle, Role, Semantics, Shadow, ShapeStyle, Span,
    Stroke, TextAlign, TextCase, TextLength, TextStyle, TextWrap,
};

const RED: Color = Color {
    r: 0.9,
    g: 0.1,
    b: 0.2,
    a: 1.0,
};
const BLUE: Color = Color {
    r: 0.1,
    g: 0.2,
    b: 0.9,
    a: 0.5,
};

#[derive(Debug)]
struct GuestTexture;

impl ExternalTexture for GuestTexture {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn gradient() -> Paint {
    Paint::Gradient(Gradient::linear(
        Point::new(0.0, 0.0),
        Point::new(10.0, 4.0),
        &[(0.0, RED), (0.4, BLUE), (1.0, RED)],
    ))
}

fn text_style() -> TextStyle {
    TextStyle::new(15.0, gradient())
        .with_font_family(FontFamily::stack([
            FontFamily::from("Inter"),
            FontFamily::Monospace,
        ]))
        .with_font_weight(650)
        .with_font_style(FontStyle::Oblique)
        .with_text_align(TextAlign::Justify)
        .with_clamp(3, true)
        .with_line_height(1.4)
        .with_letter_spacing(0.25)
        .with_raster(Raster::Pixel)
        .with_text_wrap(TextWrap::NoWrap)
        .with_font_variations(
            FontVariations::new()
                .with(FontTag::new(b"wght"), 640.0)
                .with(FontTag::new(b"opsz"), 14.0),
        )
        .with_font_features(FontFeatures::new().with(FontTag::new(b"tnum"), 1))
        .with_text_case(TextCase::Capitalize)
        .with_underline(true)
        .with_underline_offset(2.0)
        .with_underline_thickness(1.5)
        .with_underline_color(BLUE)
        .with_lang("fr-CA")
        .with_text_shadow(Shadow::new(1.0, 2.0, 3.0, BLUE).with_spread(0.5))
}

fn spans() -> Arc<[Span]> {
    let route = Location::from_segments(["docs", "wire"])
        .with_locale("fr")
        .with_fragment("top")
        .with_param("q", "bytes");
    vec![
        Span::new(
            0..4,
            Declared::default()
                .with_font_weight(700)
                .with_font_style(FontStyle::Italic)
                .with_color(RED)
                .with_font_family("Fira Code")
                .with_font_size(TextLength::Em(1.25))
                .with_letter_spacing(TextLength::SurfaceWidth(0.01))
                .with_raster(Raster::Smooth)
                .with_line_height(1.1)
                .with_text_align(TextAlign::End)
                .with_text_wrap(TextWrap::Wrap)
                .with_font_variations(FontVariations::new().with(FontTag::new(b"wdth"), 90.0))
                .with_font_features(FontFeatures::new().with(FontTag::new(b"liga"), 0))
                .with_text_case(TextCase::Upper)
                .with_underline(true)
                .with_underline_offset(TextLength::SurfaceMin(0.2))
                .with_underline_thickness(TextLength::Px(1.0))
                .with_underline_color(gradient()),
        )
        .linking_to(Destination::Route(route)),
        Span::new(4..9, Declared::default()).linking_to(Destination::anchor("intro")),
        Span::new(9..12, Declared::default()).linking_to(Destination::locale("de")),
        Span::new(12..14, Declared::default()).linking_to(Destination::External(
            Uri::parse("https://example.com/a?b=c").unwrap(),
        )),
        Span::new(
            14..15,
            Declared::default()
                .with_font_size(TextLength::SurfaceHeight(0.1))
                .with_letter_spacing(TextLength::SurfaceMax(0.3)),
        ),
    ]
    .into()
}

fn semantics() -> Semantics {
    Semantics {
        role: Role::Heading(3),
        label: Some("Settings".into()),
        link: Some(Destination::anchor("settings")),
        focused: true,
        toggled: Some(false),
        disabled: true,
        click_through: true,
        focusable: Some(Focusable {
            tab_stop: true,
            consumes: ConsumedKeys::ARROWS.with(ConsumedKeys::ENTER),
        }),
        lang: Some("es".into()),
        hidden: true,
        anchor: Some("top".into()),
        current: true,
        value: Some(NumericValue {
            now: 0.3,
            min: -1.0,
            max: 2.5,
        }),
        orientation: Some(Orientation::Vertical),
        expanded: Some(true),
        position: Some(SetPosition {
            level: 2,
            position: 4,
            size: 9,
        }),
        active_descendant: Some(41),
        controls: Some(u64::MAX),
        has_popup: Some(HasPopup::ListBox),
        keyboard_shortcut: Some("Ctrl+K".into()),
        sort: Some(SortDirection::Descending),
        multiselectable: true,
        modal: Some(Modal {
            layer: 3,
            opened_from: Some(8),
        }),
    }
}

fn element() -> Element {
    Element::new(
        ElementId(77),
        semantics(),
        "display:flex;gap:8px;",
        Rect::new(1.0, 2.0, 300.0, 40.0),
    )
    .showing(Picture {
        source: Linked {
            url: "/hero.avif".into(),
            variants: vec![(320, Arc::from("/hero-320.avif"))].into(),
        },
        width: 1280,
        height: 720,
        fit: "cover",
        priority: true,
    })
    .asking_to_scroll(Some((0.0, 120.5)))
    .arriving_within(Insets::new(48.0, 0.0, 0.0, 0.0))
    .scrolling()
    .as_primary_scroll(true)
    .fixed_in_place_of(ElementId(12))
    .with_caret(Some(Rect::new(4.0, 2.0, 1.0, 16.0)))
}

fn pixels() -> Arc<ImageData> {
    Arc::new(ImageData::new(
        (0..4 * 3 * 4u8).map(|byte| byte.wrapping_mul(7)).collect(),
        4,
        3,
    ))
}

fn every_command() -> Vec<DrawCommand> {
    let rect_style = RectStyle::filled(RED, 6.0)
        .with_radius(BorderRadius {
            top_left: 1.0,
            top_right: 2.0,
            bottom_right: 3.0,
            bottom_left: 4.0,
        })
        .with_border(Border::per_side(gradient(), 1.0, 0.0, 2.0, 0.5))
        .with_shadow(Shadow::new(0.0, 4.0, 12.0, BLUE).with_spread(-2.0));
    let stroke = Stroke::new(BLUE, 2.0)
        .with_dash(&[4.0, 2.0, 1.0], 1.5)
        .with_cap(LineCap::Square)
        .with_join(LineJoin::Bevel);
    let path = PathData::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(10.0, 0.0))
        .quad_to(Point::new(12.0, 2.0), Point::new(10.0, 4.0))
        .cubic_to(
            Point::new(8.0, 6.0),
            Point::new(2.0, 6.0),
            Point::new(0.0, 4.0),
        )
        .close();
    let path_style = PathStyle {
        fill: Some(RED.into()),
        stroke: Some(Stroke::new(RED, 1.0).with_cap(LineCap::Round)),
        shadow: Some(Shadow::new(1.0, 1.0, 2.0, RED)),
        fill_rule: FillRule::EvenOdd,
    };
    vec![
        DrawCommand::PushSurface { surface: 5 },
        DrawCommand::PushElement {
            element: Arc::new(element()),
        },
        DrawCommand::PushClip {
            rect: Rect::new(0.0, 0.0, 800.0, 600.0),
            radius: BorderRadius::all(8.0),
        },
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.1, -0.1, 1.0, 20.0, -3.5],
        },
        DrawCommand::PushLayer {
            opacity: 0.75,
            backdrop_blur: 6.0,
            blend: BlendMode::SoftLight,
            mask: LayerMask::Source,
        },
        DrawCommand::Rect {
            rect: Rect::new(10.0, 10.0, 100.0, 50.0),
            style: Arc::new(rect_style),
        },
        DrawCommand::Text {
            text: "Héllo wire!🙂".into(),
            spans: Some(spans()),
            rect: Rect::new(10.0, 70.0, 200.0, 20.0),
            style: Arc::new(text_style()),
        },
        DrawCommand::Text {
            text: "plain".into(),
            spans: None,
            rect: Rect::new(10.0, 90.0, 200.0, 20.0),
            style: Arc::new(TextStyle::new(12.0, RED)),
        },
        DrawCommand::Image {
            data: pixels(),
            rect: Rect::new(0.0, 0.0, 8.0, 6.0),
            raster: Raster::Pixel,
            fill: ImageFill::Stretch,
        },
        DrawCommand::Image {
            data: Arc::new(ImageData::linked(
                "/a.png",
                64,
                32,
                &[(16, "/a-16.png"), (32, "/a-32.png")],
            )),
            rect: Rect::new(0.0, 0.0, 64.0, 32.0),
            raster: Raster::Smooth,
            fill: ImageFill::Tile { scale: 2.0 },
        },
        DrawCommand::Image {
            data: Arc::new(ImageData::external(Arc::new(GuestTexture), 0xfeed, 16, 9)),
            rect: Rect::new(0.0, 0.0, 16.0, 9.0),
            raster: Raster::Smooth,
            fill: ImageFill::Slice(
                ImageSlice::new(Insets::new(1.0, 2.0, 3.0, 4.0)).with_scale(2.0),
            ),
        },
        DrawCommand::Line {
            p1: Point::new(0.0, 0.0),
            p2: Point::new(5.0, 5.0),
            style: Arc::new(stroke),
        },
        DrawCommand::Path {
            data: Arc::new(path),
            style: Arc::new(path_style),
        },
        DrawCommand::PopLayer,
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
        DrawCommand::PopSurface,
    ]
}

fn round_trip(
    encoder: &mut FrameEncoder,
    decoder: &mut FrameDecoder,
    commands: &[DrawCommand],
) -> Result<Vec<DrawCommand>, WireError> {
    let mut out = Vec::new();
    let bytes = encoder.encode(commands).to_vec();
    decoder.decode(&bytes, &mut out, |id| {
        encoder.pixels(id).map(<[u8]>::to_vec)
    })?;
    Ok(out)
}

fn element_of(command: &DrawCommand) -> &Arc<Element> {
    match command {
        DrawCommand::PushElement { element } => element,
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn every_command_comes_back_equal() {
    let commands = every_command();
    let decoded = round_trip(
        &mut FrameEncoder::new(),
        &mut FrameDecoder::new(),
        &commands,
    )
    .unwrap();
    assert_eq!(decoded, commands);
    assert_eq!(**element_of(&decoded[1]), element());
}

#[test]
fn images_keep_their_id_size_and_pixels() {
    let commands = every_command();
    let decoded = round_trip(
        &mut FrameEncoder::new(),
        &mut FrameDecoder::new(),
        &commands,
    )
    .unwrap();
    let images: Vec<&Arc<ImageData>> = decoded
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Image { data, .. } => Some(data),
            _ => None,
        })
        .collect();
    let [bitmap, linked, external] = images[..] else {
        panic!("three images, got {}", images.len());
    };
    assert_eq!(bitmap.pixels(), pixels().pixels());
    assert_eq!(
        (bitmap.id, bitmap.width, bitmap.height),
        (pixels().id, 4, 3)
    );
    assert_eq!(linked.linked_source().unwrap().url.as_ref(), "/a.png");
    assert_eq!(linked.linked_source().unwrap().variants.len(), 2);
    assert_eq!(
        (external.id, external.width, external.height),
        (0xfeed, 16, 9)
    );
    assert!(external.pixels().is_empty());
}

#[test]
fn a_payload_the_last_frame_sent_is_named_and_decodes_to_the_same_arc() {
    let commands = every_command();
    let (mut encoder, mut decoder) = (FrameEncoder::new(), FrameDecoder::new());
    let mut first = Vec::new();
    let first_bytes = encoder.encode(&commands).to_vec();
    decoder
        .decode(&first_bytes, &mut first, |id| {
            encoder.pixels(id).map(<[u8]>::to_vec)
        })
        .unwrap();
    let mut second = Vec::new();
    let second_bytes = encoder.encode(&commands).to_vec();
    decoder
        .decode(&second_bytes, &mut second, |_| None)
        .unwrap();
    assert_eq!(second, commands);
    assert!(
        second_bytes.len() * 4 < first_bytes.len(),
        "{} is not well under {}",
        second_bytes.len(),
        first_bytes.len()
    );
    assert!(Arc::ptr_eq(element_of(&first[1]), element_of(&second[1])));
}

#[test]
fn pixels_are_lent_once_while_frames_keep_drawing_the_image() {
    let commands = every_command();
    let (mut encoder, mut decoder) = (FrameEncoder::new(), FrameDecoder::new());
    let lent = Cell::new(0);
    let mut out = Vec::new();
    for _ in 0..3 {
        let bytes = encoder.encode(&commands).to_vec();
        decoder
            .decode(&bytes, &mut out, |id| {
                lent.set(lent.get() + 1);
                encoder.pixels(id).map(<[u8]>::to_vec)
            })
            .unwrap();
    }
    assert_eq!(lent.get(), 1);
}

#[test]
fn a_payload_dropped_for_a_frame_and_drawn_again_is_sent_again() {
    let style = Arc::new(TextStyle::new(10.0, RED));
    let other = Arc::new(TextStyle::new(20.0, BLUE));
    let text = |style: &Arc<TextStyle>, text: &str| DrawCommand::Text {
        text: text.into(),
        spans: None,
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        style: Arc::clone(style),
    };
    let (mut encoder, mut decoder) = (FrameEncoder::new(), FrameDecoder::new());
    for frame in [
        vec![text(&style, "a"), text(&other, "b")],
        vec![text(&other, "c")],
        vec![text(&style, "d"), text(&other, "e"), text(&style, "f")],
        vec![text(&style, "g")],
    ] {
        assert_eq!(
            round_trip(&mut encoder, &mut decoder, &frame).unwrap(),
            frame
        );
    }
}

#[test]
fn a_frame_the_decoder_missed_is_refused_until_the_encoder_resets() {
    let commands = every_command();
    let (mut encoder, mut decoder) = (FrameEncoder::new(), FrameDecoder::new());
    round_trip(&mut encoder, &mut decoder, &commands).unwrap();
    let _missed = encoder.encode(&commands);
    assert!(matches!(
        round_trip(&mut encoder, &mut decoder, &commands),
        Err(WireError::OutOfStep { .. })
    ));
    encoder.reset();
    assert_eq!(
        round_trip(&mut encoder, &mut decoder, &commands).unwrap(),
        commands
    );
}

#[test]
fn a_cut_or_corrupted_frame_is_an_error_and_never_a_panic() {
    let commands = every_command();
    let mut encoder = FrameEncoder::new();
    let bytes = encoder.encode(&commands).to_vec();
    let lend = |id: u64| encoder.pixels(id).map(<[u8]>::to_vec);
    let mut out = Vec::new();
    for len in 0..bytes.len() {
        let result = FrameDecoder::new().decode(&bytes[..len], &mut out, lend);
        assert!(result.is_err(), "a frame cut at {len} decoded");
        assert!(out.is_empty());
    }
    for at in 0..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0xa5;
        let _ = FrameDecoder::new().decode(&flipped, &mut out, lend);
    }
    let mut long = bytes.clone();
    long.push(0);
    assert_eq!(
        FrameDecoder::new().decode(&long, &mut out, lend),
        Err(WireError::Trailing(1))
    );
}

#[test]
fn another_version_is_refused() {
    let mut bytes = FrameEncoder::new().encode(&[]).to_vec();
    bytes[4..6].copy_from_slice(&(WIRE_VERSION + 1).to_le_bytes());
    assert_eq!(
        FrameDecoder::new().decode(&bytes, &mut Vec::new(), |_| None),
        Err(WireError::Version {
            found: WIRE_VERSION + 1
        })
    );
}

#[test]
fn pixels_the_guest_does_not_lend_are_an_error() {
    let commands = every_command();
    let bytes = FrameEncoder::new().encode(&commands).to_vec();
    assert_eq!(
        FrameDecoder::new().decode(&bytes, &mut Vec::new(), |_| None),
        Err(WireError::MissingImage(pixels().id))
    );
}

#[test]
fn an_equal_payload_rebuilt_in_a_new_arc_crosses_once_and_decodes_to_one_arc() {
    let frame = || {
        (0..3)
            .map(|row| DrawCommand::Text {
                text: format!("row {row}").into(),
                spans: None,
                rect: Rect::new(0.0, row as f32 * 20.0, 100.0, 20.0),
                style: Arc::new(text_style()),
            })
            .collect::<Vec<_>>()
    };
    let style_of = |command: &DrawCommand| match command {
        DrawCommand::Text { style, .. } => Arc::clone(style),
        other => panic!("expected text, got {other:?}"),
    };
    let (mut encoder, mut decoder) = (FrameEncoder::new(), FrameDecoder::new());
    let first = round_trip(&mut encoder, &mut decoder, &frame()).unwrap();
    let second_frame = frame();
    let second_len = encoder.encode(&second_frame).len();
    let mut second = Vec::new();
    decoder
        .decode(encoder.bytes(), &mut second, |_| None)
        .unwrap();
    assert_eq!(second, second_frame);
    assert!(Arc::ptr_eq(&style_of(&first[0]), &style_of(&first[2])));
    assert!(Arc::ptr_eq(&style_of(&first[0]), &style_of(&second[1])));
    let one_style = {
        let mut alone = FrameEncoder::new();
        alone.encode(&frame()[..1]).len()
    };
    assert!(
        second_len < one_style,
        "{second_len} resends a style ({one_style} for one row)"
    );
}
