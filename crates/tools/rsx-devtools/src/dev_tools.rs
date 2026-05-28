use std::borrow::Cow;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::dev_plugin::{DevAction, DevPlugin};
use geometry_core::Rect;
use platform_core::{Key, ModifiersState};
use renderer_core::{
    BorderRadius, Color, DrawCommand, FillStyle, RectPayload, RectStyle, TextPayload, TextStyle,
};

const BADGE_W: f32 = 100.0;
const BADGE_H: f32 = 24.0;
const MARGIN: f32 = 10.0;
const PANEL_W: f32 = 200.0;
const PANEL_H: f32 = 148.0;
const GAP: f32 = 4.0;

const BG: Color = Color::rgba(0.05, 0.05, 0.05, 1.0);
const BADGE_BG: Color = Color::rgba(0.0, 0.0, 0.0, 1.0);
const GREEN: Color = Color::rgba(0.0, 1.0, 0.4, 1.0);
const WHITE: Color = Color::rgba(0.9, 0.9, 0.9, 1.0);
const GRAY: Color = Color::rgba(0.5, 0.5, 0.5, 1.0);
const GRAY_DIM: Color = Color::rgba(0.3, 0.3, 0.3, 1.0);

// Rolling window duration — frames older than this are dropped from the count.
const FPS_WINDOW: Duration = Duration::from_secs(1);
const KEEPALIVE: Duration = Duration::from_millis(1000);

pub struct DevTools {
    frame_times: VecDeque<Instant>,
    last_fps: u32,
    panel_open: bool,
    badge_rect: Rect,
    renderer_info: Option<String>,
}

impl Default for DevTools {
    fn default() -> Self {
        Self {
            frame_times: VecDeque::new(),
            last_fps: 0,
            panel_open: false,
            badge_rect: Rect::default(),
            renderer_info: None,
        }
    }
}

impl DevTools {
    pub fn set_renderer_info(&mut self, info: &str) {
        self.renderer_info = Some(info.to_owned());
    }
}

impl DevPlugin for DevTools {
    fn on_frame<'a>(
        &mut self,
        base: &'a [DrawCommand],
        window_w: f32,
        window_h: f32,
        tree_dirty: bool,
    ) -> Cow<'a, [DrawCommand]> {
        let now = Instant::now();
        // Only sample on content frames so hardware keepalive blits don't pollute the count.
        if tree_dirty {
            self.frame_times.push_back(now);
        }
        let cutoff = now - FPS_WINDOW;
        while self.frame_times.front().is_some_and(|t| *t <= cutoff) {
            self.frame_times.pop_front();
        }
        self.last_fps = self.frame_times.len() as u32;

        let badge_x = window_w - BADGE_W - MARGIN;
        let badge_y = window_h - BADGE_H - MARGIN;
        self.badge_rect = Rect::new(badge_x, badge_y, BADGE_W, BADGE_H);

        let mut cmds = Vec::with_capacity(base.len() + 16);
        cmds.extend_from_slice(base);

        // Badge background
        cmds.push(DrawCommand::Rect(Box::new(RectPayload {
            rect: self.badge_rect,
            style: RectStyle::default()
                .with_fill(FillStyle::Solid(BADGE_BG))
                .with_radius(BorderRadius::all(4.0)),
        })));

        // Badge label
        let badge_label = format!("DEV \u{2022} {} fps", self.last_fps);
        cmds.push(DrawCommand::Text(Box::new(TextPayload {
            text: badge_label.into(),
            rect: Rect::new(badge_x + 8.0, badge_y + 5.0, BADGE_W - 16.0, BADGE_H - 10.0),
            style: TextStyle::new(12.0, GREEN),
        })));

        if self.panel_open {
            let panel_x = window_w - PANEL_W - MARGIN;
            let panel_y = badge_y - PANEL_H - GAP;

            // Panel background
            cmds.push(DrawCommand::Rect(Box::new(RectPayload {
                rect: Rect::new(panel_x, panel_y, PANEL_W, PANEL_H),
                style: RectStyle::default()
                    .with_fill(FillStyle::Solid(BG))
                    .with_radius(BorderRadius::all(8.0)),
            })));

            // Title
            cmds.push(DrawCommand::Text(Box::new(TextPayload {
                text: "rsx devtools".into(),
                rect: Rect::new(panel_x + 12.0, panel_y + 12.0, PANEL_W - 24.0, 16.0),
                style: TextStyle::new(11.0, WHITE),
            })));

            // Horizontal separator
            cmds.push(DrawCommand::Rect(Box::new(RectPayload {
                rect: Rect::new(panel_x + 12.0, panel_y + 36.0, PANEL_W - 24.0, 1.0),
                style: RectStyle::default().with_fill(FillStyle::Solid(GRAY_DIM)),
            })));

            // FPS value
            let fps_label = format!("{} fps", self.last_fps);
            cmds.push(DrawCommand::Text(Box::new(TextPayload {
                text: fps_label.into(),
                rect: Rect::new(panel_x + 12.0, panel_y + 44.0, PANEL_W - 24.0, 28.0),
                style: TextStyle::new(20.0, GREEN),
            })));

            // Renderer info line (only when set)
            let renderer_text_y = if let Some(ref info) = self.renderer_info {
                let renderer_label = format!("renderer: {}", info);
                cmds.push(DrawCommand::Text(Box::new(TextPayload {
                    text: renderer_label.into(),
                    rect: Rect::new(panel_x + 12.0, panel_y + 80.0, PANEL_W - 24.0, 16.0),
                    style: TextStyle::new(11.0, GRAY),
                })));
                96.0
            } else {
                80.0
            };

            // Keyboard shortcut hints
            cmds.push(DrawCommand::Text(Box::new(TextPayload {
                text: "ctrl+shift+b  toggle renderer".into(),
                rect: Rect::new(
                    panel_x + 12.0,
                    panel_y + renderer_text_y,
                    PANEL_W - 24.0,
                    14.0,
                ),
                style: TextStyle::new(10.0, GRAY_DIM),
            })));

            cmds.push(DrawCommand::Text(Box::new(TextPayload {
                text: "click badge  close".into(),
                rect: Rect::new(
                    panel_x + 12.0,
                    panel_y + renderer_text_y + 18.0,
                    PANEL_W - 24.0,
                    14.0,
                ),
                style: TextStyle::new(10.0, GRAY_DIM),
            })));
        }

        Cow::Owned(cmds)
    }

    fn keepalive_interval(&self) -> Option<Duration> {
        Some(KEEPALIVE)
    }

    fn on_key(&mut self, key: &Key, modifiers: ModifiersState) -> DevAction {
        if modifiers.ctrl && modifiers.shift {
            match key {
                Key::Char('b' | 'B') => return DevAction::ToggleBackend,
                Key::Char('d' | 'D') => {
                    self.panel_open = !self.panel_open;
                    return DevAction::Redraw;
                }
                _ => {}
            }
        }
        DevAction::None
    }

    fn on_pointer_pressed(&mut self, x: f32, y: f32) -> bool {
        if self.badge_rect.contains(x, y) {
            self.panel_open = !self.panel_open;
            return true;
        }
        false
    }
}
