use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::FxHasher;

use crate::DrawCommand;
use crate::style_pool::{hash_path_style, hash_rect_style, hash_text_style};

pub fn hash_draw_commands_into<H: Hasher>(cmds: &[DrawCommand], h: &mut H) {
    cmds.len().hash(h);
    for cmd in cmds {
        match cmd {
            DrawCommand::Rect { rect, style } => {
                0u8.hash(h);
                rect.x.to_bits().hash(h);
                rect.y.to_bits().hash(h);
                rect.width.to_bits().hash(h);
                rect.height.to_bits().hash(h);
                hash_rect_style(style).hash(h);
            }
            DrawCommand::Text { text, rect, style } => {
                1u8.hash(h);
                text.as_bytes().hash(h);
                rect.x.to_bits().hash(h);
                rect.y.to_bits().hash(h);
                rect.width.to_bits().hash(h);
                rect.height.to_bits().hash(h);
                hash_text_style(style).hash(h);
            }
            DrawCommand::RichText { runs, rect, base } => {
                10u8.hash(h);
                for run in runs.iter() {
                    run.text.as_bytes().hash(h);
                    run.weight.hash(h);
                    run.italic.hash(h);
                    run.color.to_rgba8().hash(h);
                }
                rect.x.to_bits().hash(h);
                rect.y.to_bits().hash(h);
                rect.width.to_bits().hash(h);
                rect.height.to_bits().hash(h);
                hash_text_style(base).hash(h);
            }
            DrawCommand::Image { data, rect, filter } => {
                2u8.hash(h);
                data.id.hash(h);
                rect.x.to_bits().hash(h);
                rect.y.to_bits().hash(h);
                rect.width.to_bits().hash(h);
                rect.height.to_bits().hash(h);
                (*filter as u8).hash(h);
            }
            DrawCommand::Line { p1, p2, style } => {
                3u8.hash(h);
                p1.x.to_bits().hash(h);
                p1.y.to_bits().hash(h);
                p2.x.to_bits().hash(h);
                p2.y.to_bits().hash(h);
                style.width.to_bits().hash(h);
            }
            DrawCommand::Path { data, style } => {
                4u8.hash(h);
                (Arc::as_ptr(data) as usize).hash(h);
                hash_path_style(style).hash(h);
            }
            DrawCommand::PushClip { rect, radius } => {
                5u8.hash(h);
                rect.x.to_bits().hash(h);
                rect.y.to_bits().hash(h);
                rect.width.to_bits().hash(h);
                rect.height.to_bits().hash(h);
                radius.top_left.to_bits().hash(h);
                radius.top_right.to_bits().hash(h);
                radius.bottom_right.to_bits().hash(h);
                radius.bottom_left.to_bits().hash(h);
            }
            DrawCommand::PopClip => 6u8.hash(h),
            DrawCommand::PushMatrix { matrix } => {
                7u8.hash(h);
                for v in matrix {
                    v.to_bits().hash(h);
                }
            }
            DrawCommand::PopMatrix => 8u8.hash(h),
            DrawCommand::PushLayer {
                opacity,
                backdrop_blur,
            } => {
                9u8.hash(h);
                opacity.to_bits().hash(h);
                backdrop_blur.to_bits().hash(h);
            }
            DrawCommand::PopLayer => 11u8.hash(h),
        }
    }
}

pub fn hash_draw_commands(cmds: &[DrawCommand]) -> u64 {
    let mut h = FxHasher::default();
    hash_draw_commands_into(cmds, &mut h);
    h.finish()
}

pub fn hash_pod_slice<T: bytemuck::Pod>(data: &[T]) -> u64 {
    let bytes: &[u8] = bytemuck::cast_slice(data);
    let mut hasher = FxHasher::default();
    bytes.hash(&mut hasher);
    hasher.finish()
}
