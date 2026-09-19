//! Shared by the tests of this crate and of every backend.

use std::sync::Arc;

use geometry_core::Rect;

use crate::{
    BlendMode, BorderRadius, Color, DrawCommand, Element, ElementId, ImageData, ImageFill,
    ImageSlice, Insets, Raster, RectStyle, Semantics, ShapeStyle,
};

pub struct Scenario {
    pub name: &'static str,
    pub size: (u32, u32),
    pub old: Vec<DrawCommand>,
    pub new: Vec<DrawCommand>,
    pub plan: Plan,
}

pub enum Plan {
    Damage(Vec<Rect>),
    Scroll {
        clip: Rect,
        delta: (i32, i32),
        exposed: Rect,
        extra: Vec<Rect>,
    },
}

const SURFACE: (u32, u32) = (3840, 2160);

fn open(id: u64) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(Element::new(
            ElementId(id),
            Semantics::group(),
            "",
            Rect::default(),
        )),
    }
}

fn painted(x: f32, y: f32, width: f32, height: f32, color: Color) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, y, width, height),
        style: Arc::new(RectStyle::default().with_fill(color)),
    }
}

fn filled(x: f32, y: f32, width: f32, height: f32) -> DrawCommand {
    painted(x, y, width, height, Color::from_rgb_u8(40, 120, 200))
}

fn backdrop(x: f32, y: f32, width: f32, height: f32) -> DrawCommand {
    painted(x, y, width, height, Color::from_rgb_u8(30, 30, 40))
}

fn clip(x: f32, y: f32, width: f32, height: f32, radius: f32) -> DrawCommand {
    DrawCommand::PushClip {
        rect: Rect::new(x, y, width, height),
        radius: BorderRadius::all(radius),
    }
}

fn layer(opacity: f32) -> Vec<DrawCommand> {
    vec![
        open(1),
        DrawCommand::PushLayer {
            opacity,
            backdrop_blur: 0.0,
            blend: BlendMode::Normal,
        },
        filled(100.0, 100.0, 40.0, 40.0),
        DrawCommand::PopLayer,
        DrawCommand::PopElement,
    ]
}

fn blended_layer(blend: BlendMode) -> Vec<DrawCommand> {
    vec![
        backdrop(80.0, 80.0, 200.0, 200.0),
        open(1),
        DrawCommand::PushLayer {
            opacity: 1.0,
            backdrop_blur: 0.0,
            blend,
        },
        filled(100.0, 100.0, 40.0, 40.0),
        DrawCommand::PopLayer,
        DrawCommand::PopElement,
    ]
}

/// Four pixels of four colours, so a tile period or a slice line shows up in the pixels.
fn quadrants() -> Arc<ImageData> {
    let pixels = [
        [220, 40, 40, 255],
        [40, 200, 60, 255],
        [40, 60, 220, 255],
        [230, 200, 40, 255],
    ];
    Arc::new(ImageData::new(pixels.concat(), 2, 2))
}

fn picture(rect: Rect, fill: ImageFill) -> Vec<DrawCommand> {
    vec![
        backdrop(0.0, 0.0, 1600.0, 1000.0),
        open(1),
        DrawCommand::Image {
            data: quadrants(),
            rect,
            raster: Raster::Pixel,
            fill,
        },
        DrawCommand::PopElement,
    ]
}

fn rounded(radius: f32) -> Vec<DrawCommand> {
    vec![
        open(1),
        clip(200.0, 200.0, 120.0, 40.0, radius),
        filled(200.0, 200.0, 120.0, 40.0),
        DrawCommand::PopClip,
        DrawCommand::PopElement,
    ]
}

fn column(cards: &[(u64, f32)]) -> Vec<DrawCommand> {
    let mut commands = vec![open(100)];
    for &(id, y) in cards {
        commands.extend([
            open(id),
            filled(300.0, y, 200.0, 40.0),
            DrawCommand::PopElement,
        ]);
    }
    commands.push(DrawCommand::PopElement);
    commands
}

fn chip(width: f32) -> Vec<DrawCommand> {
    vec![
        open(1),
        clip(400.0, 400.0, width, 24.0, 12.0),
        filled(400.0, 400.0, width, 24.0),
        filled(408.0, 404.0, width - 16.0, 16.0),
        DrawCommand::PopClip,
        DrawCommand::PopElement,
    ]
}

fn resizing_clip(width: f32, content_x: f32) -> Vec<DrawCommand> {
    vec![
        open(1),
        clip(1000.0, 200.0, width, 100.0, 0.0),
        filled(content_x, 200.0, 300.0, 100.0),
        DrawCommand::PopClip,
        DrawCommand::PopElement,
    ]
}

fn over_background(card: Color) -> Vec<DrawCommand> {
    vec![
        backdrop(0.0, 0.0, 3840.0, 2160.0),
        open(1),
        painted(2000.0, 1000.0, 100.0, 50.0, card),
        DrawCommand::PopElement,
    ]
}

fn spilling_layer(opacity: f32) -> Vec<DrawCommand> {
    vec![
        open(1),
        clip(600.0, 600.0, 200.0, 100.0, 0.0),
        DrawCommand::PushLayer {
            opacity,
            backdrop_blur: 0.0,
            blend: BlendMode::Normal,
        },
        filled(650.0, 650.0, 300.0, 200.0),
        DrawCommand::PopLayer,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
    ]
}

fn nested_layers(opacity: f32) -> Vec<DrawCommand> {
    vec![
        open(1),
        clip(900.0, 200.0, 200.0, 120.0, 24.0),
        DrawCommand::PushLayer {
            opacity,
            backdrop_blur: 0.0,
            blend: BlendMode::Normal,
        },
        filled(880.0, 180.0, 240.0, 160.0),
        DrawCommand::PushLayer {
            opacity: 0.8,
            backdrop_blur: 0.0,
            blend: BlendMode::Normal,
        },
        painted(860.0, 240.0, 120.0, 40.0, Color::from_rgb_u8(220, 200, 60)),
        DrawCommand::PopLayer,
        DrawCommand::PopLayer,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
    ]
}

fn window(offset: f32, tooltip_y: Option<f32>) -> Vec<DrawCommand> {
    let mut commands = vec![
        open(1),
        backdrop(0.0, 0.0, 300.0, 2160.0),
        backdrop(300.0, 0.0, 3540.0, 80.0),
        open(2),
        clip(320.0, 100.0, 1200.0, 1000.0, 0.0),
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 320.0, 100.0 - offset],
        },
        open(3),
    ];
    for row in 0..30 {
        let shade = if row % 2 == 0 { 90 } else { 160 };
        commands.push(painted(
            0.0,
            row as f32 * 50.0,
            1200.0,
            40.0,
            Color::from_rgb_u8(shade, 140, 220),
        ));
    }
    commands.extend([
        DrawCommand::PopElement,
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
    ]);
    if let Some(y) = tooltip_y {
        commands.extend([
            open(4),
            filled(1600.0, y, 200.0, 60.0),
            DrawCommand::PopElement,
        ]);
    }
    commands.push(DrawCommand::PopElement);
    commands
}

/// A shell's notification list: the panel's rounded background is drawn under the viewport and reaches past it on every side, so scrolling the list leaves not a pixel of it different.
fn notifications(offset: f32) -> Vec<DrawCommand> {
    let mut commands = vec![
        open(1),
        DrawCommand::Rect {
            rect: Rect::new(2800.0, 100.0, 900.0, 1200.0),
            style: Arc::new(RectStyle::filled(Color::from_rgb_u8(24, 24, 32), 24.0)),
        },
        open(2),
        clip(2840.0, 200.0, 820.0, 1000.0, 0.0),
        DrawCommand::PushMatrix {
            matrix: [1.0, 0.0, 0.0, 1.0, 2840.0, 200.0 - offset],
        },
        open(3),
    ];
    for row in 0..40 {
        let shade = if row % 2 == 0 { 70 } else { 120 };
        commands.push(painted(
            0.0,
            row as f32 * 60.0,
            820.0,
            50.0,
            Color::from_rgb_u8(shade, 100, 170),
        ));
    }
    commands.extend([
        DrawCommand::PopElement,
        DrawCommand::PopMatrix,
        DrawCommand::PopClip,
        DrawCommand::PopElement,
        DrawCommand::PopElement,
    ]);
    commands
}

pub fn all() -> Vec<Scenario> {
    let moved_cards = vec![
        Rect::new(300.0, 148.0, 200.0, 40.0),
        Rect::new(300.0, 196.0, 200.0, 40.0),
        Rect::new(300.0, 244.0, 200.0, 40.0),
    ];
    let three = column(&[(1, 100.0), (2, 148.0), (3, 196.0)]);
    let four = column(&[(1, 100.0), (4, 148.0), (2, 196.0), (3, 244.0)]);
    let list = Rect::new(320.0, 100.0, 1200.0, 1000.0);
    vec![
        Scenario {
            name: "a layer animating its opacity",
            size: SURFACE,
            old: layer(0.5),
            new: layer(0.6),
            plan: Plan::Damage(vec![Rect::new(100.0, 100.0, 40.0, 40.0)]),
        },
        Scenario {
            name: "a layer changing its blend mode",
            size: SURFACE,
            old: blended_layer(BlendMode::Multiply),
            new: blended_layer(BlendMode::Screen),
            plan: Plan::Damage(vec![Rect::new(100.0, 100.0, 40.0, 40.0)]),
        },
        Scenario {
            name: "a tiled picture changing its scale",
            size: SURFACE,
            old: picture(
                Rect::new(500.0, 500.0, 64.0, 48.0),
                ImageFill::Tile { scale: 4.0 },
            ),
            new: picture(
                Rect::new(500.0, 500.0, 64.0, 48.0),
                ImageFill::Tile { scale: 8.0 },
            ),
            plan: Plan::Damage(vec![Rect::new(500.0, 500.0, 64.0, 48.0)]),
        },
        Scenario {
            name: "a nine-slice picture changing its insets",
            size: SURFACE,
            old: picture(
                Rect::new(700.0, 500.0, 90.0, 60.0),
                ImageFill::Slice(ImageSlice::new(Insets::all(1.0)).with_scale(8.0)),
            ),
            new: picture(
                Rect::new(700.0, 500.0, 90.0, 60.0),
                ImageFill::Slice(ImageSlice::new(Insets::new(1.0, 0.5, 1.0, 0.5)).with_scale(8.0)),
            ),
            plan: Plan::Damage(vec![Rect::new(700.0, 500.0, 90.0, 60.0)]),
        },
        Scenario {
            name: "a rounded clip changing its radius",
            size: SURFACE,
            old: rounded(8.0),
            new: rounded(16.0),
            plan: Plan::Damage(vec![Rect::new(200.0, 200.0, 120.0, 40.0)]),
        },
        Scenario {
            name: "a card inserted mid-list",
            size: SURFACE,
            old: three.clone(),
            new: four.clone(),
            plan: Plan::Damage(moved_cards.clone()),
        },
        Scenario {
            name: "a card removed mid-list",
            size: SURFACE,
            old: four,
            new: three,
            plan: Plan::Damage(moved_cards),
        },
        Scenario {
            name: "a rounded chip widening with its text",
            size: SURFACE,
            old: chip(80.0),
            new: chip(84.0),
            plan: Plan::Damage(vec![Rect::new(400.0, 400.0, 84.0, 24.0)]),
        },
        Scenario {
            name: "a clip resizing with the content it cuts",
            size: SURFACE,
            old: resizing_clip(200.0, 1000.0),
            new: resizing_clip(160.0, 1040.0),
            plan: Plan::Damage(vec![Rect::new(1000.0, 200.0, 200.0, 100.0)]),
        },
        Scenario {
            name: "a card changing over a full-window background",
            size: SURFACE,
            old: over_background(Color::from_rgb_u8(40, 120, 200)),
            new: over_background(Color::from_rgb_u8(200, 120, 40)),
            plan: Plan::Damage(vec![Rect::new(2000.0, 1000.0, 100.0, 50.0)]),
        },
        Scenario {
            name: "a translucent layer spilling past its clip",
            size: SURFACE,
            old: spilling_layer(0.5),
            new: spilling_layer(0.7),
            plan: Plan::Damage(vec![Rect::new(650.0, 650.0, 150.0, 50.0)]),
        },
        Scenario {
            name: "a layer nested in a rounded clip",
            size: SURFACE,
            old: nested_layers(0.5),
            new: nested_layers(0.7),
            plan: Plan::Damage(vec![Rect::new(900.0, 200.0, 200.0, 120.0)]),
        },
        Scenario {
            name: "a list scrolling while a tooltip outside it is dismissed",
            size: SURFACE,
            old: window(0.0, Some(300.0)),
            new: window(40.0, None),
            plan: Plan::Scroll {
                clip: list,
                delta: (0, -40),
                exposed: Rect::new(320.0, 1060.0, 1200.0, 40.0),
                extra: vec![Rect::new(1600.0, 300.0, 200.0, 60.0)],
            },
        },
        Scenario {
            name: "a list scrolling while a tooltip outside it moves",
            size: SURFACE,
            old: window(0.0, Some(300.0)),
            new: window(10.0, Some(400.0)),
            plan: Plan::Scroll {
                clip: list,
                delta: (0, -10),
                exposed: Rect::new(320.0, 1090.0, 1200.0, 10.0),
                extra: vec![
                    Rect::new(1600.0, 300.0, 200.0, 60.0),
                    Rect::new(1600.0, 400.0, 200.0, 60.0),
                ],
            },
        },
        Scenario {
            name: "a notification list scrolling over its panel's background",
            size: SURFACE,
            old: notifications(0.0),
            new: notifications(40.0),
            plan: Plan::Scroll {
                clip: Rect::new(2840.0, 200.0, 820.0, 1000.0),
                delta: (0, -40),
                exposed: Rect::new(2840.0, 1160.0, 820.0, 40.0),
                extra: vec![],
            },
        },
    ]
}
