use std::borrow::Cow;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use devtools_core::{DevAction, DevPlugin};
use geometry_core::Rect;
use platform_core::{Key, ModifiersState};
use renderer_core::{
    BorderRadius, Color, DrawCommand, Paint, RectStyle, ShapeStyle, Stroke, TextStyle,
};

fn rect_command(rect: Rect, style: RectStyle) -> DrawCommand {
    DrawCommand::Rect {
        rect,
        style: std::sync::Arc::new(style),
    }
}

fn text_command(text: std::sync::Arc<str>, rect: Rect, style: TextStyle) -> DrawCommand {
    DrawCommand::Text {
        text,
        rect,
        style: std::sync::Arc::new(style),
    }
}

const BADGE_WIDTH: f32 = 100.0;
const BADGE_HEIGHT: f32 = 24.0;
const MARGIN: f32 = 10.0;
const PANEL_WIDTH: f32 = 200.0;
const PANEL_HEIGHT: f32 = 178.0;
const GAP: f32 = 4.0;

const INSPECTOR_WIDTH: f32 = 300.0;
const INSPECTOR_BACKGROUND: Color = Color::rgba(0.08, 0.08, 0.12, 0.92);
const INSPECTOR_SELECTION_BACKGROUND: Color = Color::rgba(0.2, 0.4, 0.8, 0.25);
const HIGHLIGHT_FILL: Color = Color::rgba(0.2, 0.5, 1.0, 0.18);
const HIGHLIGHT_BORDER: Color = Color::rgba(0.3, 0.6, 1.0, 0.85);
const ROW_HEIGHT: f32 = 20.0;
const INSPECTOR_HEADER_HEIGHT: f32 = 32.0;

const PANEL_BACKGROUND: Color = Color::rgba(0.05, 0.05, 0.05, 0.75);
const BADGE_BACKGROUND: Color = Color::rgba(0.0, 0.0, 0.0, 0.70);
const BACKDROP_BLUR_SIGMA: f32 = 12.0;
const GREEN: Color = Color::rgba(0.0, 1.0, 0.4, 1.0);
const WHITE: Color = Color::rgba(0.9, 0.9, 0.9, 1.0);
const GRAY: Color = Color::rgba(0.5, 0.5, 0.5, 1.0);
const GRAY_DIM: Color = Color::rgba(0.3, 0.3, 0.3, 1.0);

// Rolling window duration — frames older than this are dropped from the count.
const FPS_WINDOW: Duration = Duration::from_secs(1);
const KEEPALIVE_INTERVAL: Duration = Duration::from_millis(1000);

struct CachedNode {
    id: u64,
    name: &'static str,
    rect: Rect,
    depth: usize,
}

pub struct DevTools {
    frame_times: VecDeque<Instant>,
    last_fps: u32,
    panel_open: bool,
    badge_rect: Rect,
    renderer_info: Option<String>,
    build_error: Option<String>,
    inspector_open: bool,
    selected_node: Option<u64>,
    nodes: Vec<CachedNode>,
    inspector_rect: Rect,
    frame_time_millis: f32,
    node_count: usize,
}

impl Default for DevTools {
    fn default() -> Self {
        Self {
            frame_times: VecDeque::new(),
            last_fps: 0,
            panel_open: false,
            badge_rect: Rect::default(),
            renderer_info: None,
            build_error: None,
            inspector_open: false,
            selected_node: None,
            nodes: Vec::new(),
            inspector_rect: Rect::default(),
            frame_time_millis: 0.0,
            node_count: 0,
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

        self.frame_time_millis = if self.frame_times.len() >= 2 {
            let oldest = self.frame_times[0];
            let newest = self.frame_times[self.frame_times.len() - 1];
            let elapsed_ms = newest.duration_since(oldest).as_secs_f32() * 1000.0;
            elapsed_ms / (self.frame_times.len() - 1) as f32
        } else {
            0.0
        };

        let badge_x = window_w - BADGE_WIDTH - MARGIN;
        let badge_y = window_h - BADGE_HEIGHT - MARGIN;
        self.badge_rect = Rect::new(badge_x, badge_y, BADGE_WIDTH, BADGE_HEIGHT);

        let mut cmds = Vec::with_capacity(base.len() + 16);
        cmds.extend_from_slice(base);

        if self.inspector_open {
            // Selected node highlight drawn on the canvas, behind the inspector panel.
            if let Some(selected_id) = self.selected_node
                && let Some(node) = self.nodes.iter().find(|n| n.id == selected_id)
                && node.rect.width > 0.0
                && node.rect.height > 0.0
            {
                cmds.push(rect_command(
                    node.rect,
                    RectStyle::default()
                        .with_fill(Paint::Solid(HIGHLIGHT_FILL))
                        .with_stroke(Stroke::new(HIGHLIGHT_BORDER, 1.5)),
                ));
            }

            let panel_rect = Rect::new(0.0, 0.0, INSPECTOR_WIDTH, window_h);
            self.inspector_rect = panel_rect;
            cmds.push(rect_command(
                panel_rect,
                RectStyle::default().with_fill(Paint::Solid(INSPECTOR_BACKGROUND)),
            ));

            let header_text = format!("Inspector  {} nodes", self.nodes.len());
            cmds.push(text_command(
                header_text.into(),
                Rect::new(12.0, 8.0, INSPECTOR_WIDTH - 24.0, 18.0),
                TextStyle::new(12.0, WHITE),
            ));
            cmds.push(rect_command(
                Rect::new(0.0, INSPECTOR_HEADER_HEIGHT, INSPECTOR_WIDTH, 1.0),
                RectStyle::default().with_fill(Paint::Solid(GRAY_DIM)),
            ));

            let max_visible = ((window_h - INSPECTOR_HEADER_HEIGHT) / ROW_HEIGHT) as usize;
            for (i, node) in self.nodes.iter().take(max_visible).enumerate() {
                let row_y = INSPECTOR_HEADER_HEIGHT + i as f32 * ROW_HEIGHT;
                let is_selected = self.selected_node == Some(node.id);

                if is_selected {
                    cmds.push(rect_command(
                        Rect::new(0.0, row_y, INSPECTOR_WIDTH, ROW_HEIGHT),
                        RectStyle::default()
                            .with_fill(Paint::Solid(INSPECTOR_SELECTION_BACKGROUND)),
                    ));
                }

                let indent = node.depth as f32 * 8.0;
                let r = node.rect;
                let label = format!("{}  {:.0}\u{00d7}{:.0}", node.name, r.width, r.height);
                cmds.push(text_command(
                    label.into(),
                    Rect::new(
                        12.0 + indent,
                        row_y + 3.0,
                        INSPECTOR_WIDTH - 24.0 - indent,
                        ROW_HEIGHT - 6.0,
                    ),
                    TextStyle::new(10.0, if is_selected { WHITE } else { GRAY }),
                ));
            }
        }

        // Badge wrapped in a clip + backdrop-blur layer so the semi-transparent fill samples a blurred copy of the underlying content.
        cmds.push(DrawCommand::PushClip {
            rect: self.badge_rect,
            radius: BorderRadius::all(4.0),
        });
        cmds.push(DrawCommand::PushLayer {
            opacity: 1.0,
            backdrop_blur: BACKDROP_BLUR_SIGMA,
        });

        cmds.push(rect_command(
            self.badge_rect,
            RectStyle::default()
                .with_fill(Paint::Solid(BADGE_BACKGROUND))
                .with_radius(BorderRadius::all(4.0)),
        ));

        let badge_label = format!("DEV \u{2022} {} fps", self.last_fps);
        cmds.push(text_command(
            badge_label.into(),
            Rect::new(
                badge_x + 8.0,
                badge_y + 5.0,
                BADGE_WIDTH - 16.0,
                BADGE_HEIGHT - 10.0,
            ),
            TextStyle::new(12.0, GREEN),
        ));

        cmds.push(DrawCommand::PopLayer);
        cmds.push(DrawCommand::PopClip);

        if self.panel_open {
            let panel_x = window_w - PANEL_WIDTH - MARGIN;
            let panel_y = badge_y - PANEL_HEIGHT - GAP;

            cmds.push(DrawCommand::PushClip {
                rect: Rect::new(panel_x, panel_y, PANEL_WIDTH, PANEL_HEIGHT),
                radius: BorderRadius::all(8.0),
            });
            cmds.push(DrawCommand::PushLayer {
                opacity: 1.0,
                backdrop_blur: BACKDROP_BLUR_SIGMA,
            });

            cmds.push(rect_command(
                Rect::new(panel_x, panel_y, PANEL_WIDTH, PANEL_HEIGHT),
                RectStyle::default()
                    .with_fill(Paint::Solid(PANEL_BACKGROUND))
                    .with_radius(BorderRadius::all(8.0)),
            ));

            cmds.push(text_command(
                "rsx devtools".into(),
                Rect::new(panel_x + 12.0, panel_y + 12.0, PANEL_WIDTH - 24.0, 16.0),
                TextStyle::new(11.0, WHITE),
            ));

            cmds.push(rect_command(
                Rect::new(panel_x + 12.0, panel_y + 36.0, PANEL_WIDTH - 24.0, 1.0),
                RectStyle::default().with_fill(Paint::Solid(GRAY_DIM)),
            ));

            let fps_label = format!("{} fps", self.last_fps);
            cmds.push(text_command(
                fps_label.into(),
                Rect::new(panel_x + 12.0, panel_y + 44.0, PANEL_WIDTH - 24.0, 28.0),
                TextStyle::new(20.0, GREEN),
            ));

            let frame_time_label = format!(
                "{:.1} ms/frame  {} nodes",
                self.frame_time_millis, self.node_count
            );
            cmds.push(text_command(
                frame_time_label.into(),
                Rect::new(panel_x + 12.0, panel_y + 70.0, PANEL_WIDTH - 24.0, 14.0),
                TextStyle::new(10.0, GRAY),
            ));

            let renderer_text_y = if let Some(ref info) = self.renderer_info {
                let renderer_label = format!("renderer: {}", info);
                cmds.push(text_command(
                    renderer_label.into(),
                    Rect::new(panel_x + 12.0, panel_y + 90.0, PANEL_WIDTH - 24.0, 16.0),
                    TextStyle::new(11.0, GRAY),
                ));
                108.0
            } else {
                90.0
            };

            cmds.push(text_command(
                "ctrl+shift+b  toggle renderer".into(),
                Rect::new(
                    panel_x + 12.0,
                    panel_y + renderer_text_y,
                    PANEL_WIDTH - 24.0,
                    14.0,
                ),
                TextStyle::new(10.0, GRAY_DIM),
            ));

            cmds.push(text_command(
                "ctrl+shift+i  inspector".into(),
                Rect::new(
                    panel_x + 12.0,
                    panel_y + renderer_text_y + 16.0,
                    PANEL_WIDTH - 24.0,
                    14.0,
                ),
                TextStyle::new(10.0, GRAY_DIM),
            ));

            cmds.push(text_command(
                "click badge  close".into(),
                Rect::new(
                    panel_x + 12.0,
                    panel_y + renderer_text_y + 32.0,
                    PANEL_WIDTH - 24.0,
                    14.0,
                ),
                TextStyle::new(10.0, GRAY_DIM),
            ));

            cmds.push(DrawCommand::PopLayer);
            cmds.push(DrawCommand::PopClip);
        }

        // Error banner — shown on top of everything when a build fails
        if let Some(ref error_msg) = self.build_error {
            const BANNER_PAD: f32 = 16.0;
            const BANNER_LINE_HEIGHT: f32 = 16.0;
            const ERROR_BACKGROUND: Color = Color::rgba(0.7, 0.1, 0.1, 0.92);
            const ERROR_TEXT: Color = Color::rgba(1.0, 0.9, 0.9, 1.0);
            const TITLE_COLOR: Color = Color::rgba(1.0, 0.5, 0.5, 1.0);

            let lines: Vec<&str> = error_msg.lines().take(20).collect();
            let banner_h = BANNER_PAD * 2.0
                + BANNER_LINE_HEIGHT
                + lines.len() as f32 * (BANNER_LINE_HEIGHT + 2.0);
            let banner_rect = Rect::new(0.0, 0.0, window_w, banner_h);

            cmds.push(rect_command(
                banner_rect,
                RectStyle::default().with_fill(Paint::Solid(ERROR_BACKGROUND)),
            ));
            cmds.push(text_command(
                "Build failed".into(),
                Rect::new(
                    BANNER_PAD,
                    BANNER_PAD,
                    window_w - BANNER_PAD * 2.0,
                    BANNER_LINE_HEIGHT,
                ),
                TextStyle::new(13.0, TITLE_COLOR),
            ));
            for (i, line) in lines.iter().enumerate() {
                let y =
                    BANNER_PAD + BANNER_LINE_HEIGHT + 4.0 + i as f32 * (BANNER_LINE_HEIGHT + 2.0);
                cmds.push(text_command(
                    (*line).to_string().into(),
                    Rect::new(
                        BANNER_PAD,
                        y,
                        window_w - BANNER_PAD * 2.0,
                        BANNER_LINE_HEIGHT,
                    ),
                    TextStyle::new(11.0, ERROR_TEXT),
                ));
            }
        }

        Cow::Owned(cmds)
    }

    fn keepalive_interval(&self) -> Option<Duration> {
        Some(KEEPALIVE_INTERVAL)
    }

    fn on_key(&mut self, key: &Key, modifiers: ModifiersState) -> DevAction {
        if modifiers.is_ctrl && modifiers.is_shift {
            match key {
                Key::Char('b' | 'B') => return DevAction::ToggleBackend,
                Key::Char('d' | 'D') => {
                    self.panel_open = !self.panel_open;
                    return DevAction::Redraw;
                }
                Key::Char('i' | 'I') => {
                    self.inspector_open = !self.inspector_open;
                    return DevAction::Redraw;
                }
                _ => {}
            }
        }
        DevAction::None
    }

    fn on_pointer_pressed(&mut self, x: f32, y: f32) -> bool {
        if self.inspector_open && self.inspector_rect.contains(x, y) {
            let list_y = y - INSPECTOR_HEADER_HEIGHT;
            if list_y >= 0.0 {
                let idx = (list_y / ROW_HEIGHT) as usize;
                if let Some(node) = self.nodes.get(idx) {
                    self.selected_node = Some(node.id);
                }
            }
            return true;
        }

        // Canvas click with inspector open: select the deepest node under the cursor.
        if self.inspector_open {
            let clicked = self
                .nodes
                .iter()
                .filter(|n| n.rect.width > 0.0 && n.rect.height > 0.0 && n.rect.contains(x, y))
                .max_by_key(|n| n.depth);
            if let Some(node) = clicked {
                self.selected_node = Some(node.id);
                return true;
            }
        }

        if self.badge_rect.contains(x, y) {
            self.panel_open = !self.panel_open;
            return true;
        }
        false
    }

    fn set_build_error(&mut self, error: Option<String>) {
        self.build_error = error;
    }

    fn on_tree(&mut self, tree: &dyn devtools_core::DevTreeView) {
        self.node_count = tree.node_count();
        self.nodes.clear();
        tree.for_each_node(&mut |info| {
            self.nodes.push(CachedNode {
                id: info.id,
                name: info.name,
                rect: info.rect,
                depth: info.depth,
            });
        });
    }
}
