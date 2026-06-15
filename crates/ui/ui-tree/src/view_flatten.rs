use std::sync::Arc;

use renderer_core::DrawCommand;
use renderer_core::{hash_path_style, hash_rect_style, hash_text_style};
use rustc_hash::{FxHashMap, FxHasher};

use crate::render_node::RenderNode;

/// Hashes a contiguous range of draw commands into a single content fingerprint.
/// Inline geometry and style handles are hashed by value; the remaining `Arc`-backed payloads (text, path, image data) are identified by pointer/length, which is stable across frames when reused.
fn range_hash(cmds: &[DrawCommand]) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut h = FxHasher::default();
    for cmd in cmds {
        match cmd {
            DrawCommand::Rect { rect, style } => {
                0u8.hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                hash_rect_style(style).hash(&mut h);
            }
            DrawCommand::Text { text, rect, style } => {
                1u8.hash(&mut h);
                text.len().hash(&mut h);
                (text.as_ptr() as usize).hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                hash_text_style(style).hash(&mut h);
            }
            DrawCommand::Image { data, rect, filter } => {
                2u8.hash(&mut h);
                (Arc::as_ptr(data) as usize).hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                (*filter as u8).hash(&mut h);
            }
            DrawCommand::Line { p1, p2, style } => {
                3u8.hash(&mut h);
                p1.x.to_bits().hash(&mut h);
                p1.y.to_bits().hash(&mut h);
                p2.x.to_bits().hash(&mut h);
                p2.y.to_bits().hash(&mut h);
                style.width.to_bits().hash(&mut h);
            }
            DrawCommand::Path { data, style } => {
                4u8.hash(&mut h);
                (Arc::as_ptr(data) as usize).hash(&mut h);
                hash_path_style(style).hash(&mut h);
            }
            DrawCommand::PushClip { rect, radius } => {
                5u8.hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                radius.top_left.to_bits().hash(&mut h);
                radius.top_right.to_bits().hash(&mut h);
                radius.bottom_right.to_bits().hash(&mut h);
                radius.bottom_left.to_bits().hash(&mut h);
            }
            DrawCommand::PopClip => 6u8.hash(&mut h),
            DrawCommand::PushMatrix { matrix } => {
                7u8.hash(&mut h);
                for v in matrix {
                    v.to_bits().hash(&mut h);
                }
            }
            DrawCommand::PopMatrix => 8u8.hash(&mut h),
            DrawCommand::PushLayer {
                opacity,
                backdrop_blur,
            } => {
                9u8.hash(&mut h);
                opacity.to_bits().hash(&mut h);
                backdrop_blur.to_bits().hash(&mut h);
            }
            DrawCommand::PopLayer => 10u8.hash(&mut h),
            #[cfg(target_os = "android")]
            DrawCommand::AndroidHardwareBufferImage {
                handle,
                rect,
                filter,
                ..
            } => {
                11u8.hash(&mut h);
                handle.hash(&mut h);
                rect.x.to_bits().hash(&mut h);
                rect.y.to_bits().hash(&mut h);
                rect.width.to_bits().hash(&mut h);
                rect.height.to_bits().hash(&mut h);
                (*filter as u8).hash(&mut h);
            }
        }
    }
    h.finish()
}

/// `key_cache` maps `node_key -> (range_start, range_len, subtree_hash)` for keyed structural nodes. The hash fingerprints the commands a keyed subtree produced last frame; it is recomputed and refreshed here so future frames can short-circuit identical subtrees. Correctness does not depend on the cache: it is advisory infrastructure layered on top of the existing per-slot `emit_cmd!` comparison.
pub fn flatten_view(
    root: RenderNode,
    out: &mut Vec<DrawCommand>,
    stack: &mut Vec<RenderNode>,
    key_cache: &mut FxHashMap<u64, (usize, usize, u64)>,
) -> bool {
    stack.clear();
    stack.push(root);
    let mut pos: usize = 0;
    let mut changed = false;

    // Deferred close markers for keyed nodes: `(stack_len_when_done, node_key, range_start)`. A marker fires once the main stack has drained back to `stack_len_when_done`, i.e. when all of the keyed node's children have been flattened. Keyed nodes nest, so this stays correctly ordered as a LIFO.
    let mut pending_keys: Vec<(usize, u64, usize)> = Vec::new();

    // emit_cmd checks the slot at `pos` before overwriting to avoid marking changed spuriously
    macro_rules! emit_cmd {
        ($cmd:expr) => {{
            let cmd = $cmd;
            if pos < out.len() {
                if out[pos] != cmd {
                    out[pos] = cmd;
                    changed = true;
                }
            } else {
                out.push(cmd);
                changed = true;
            }
            pos += 1;
        }};
    }

    // Records a deferred close for a keyed structural node so its range hash is refreshed once its children are flattened. `stack` is captured at the point children are about to be pushed, so the marker fires when the stack returns to its current length.
    macro_rules! defer_keyed_close {
        ($node_key:expr) => {{
            if $node_key != 0 {
                pending_keys.push((stack.len(), $node_key, pos));
            }
        }};
    }

    while let Some(node) = stack.pop() {
        // Fire any keyed closes whose subtree has fully drained off the stack.
        while let Some(&(done_len, node_key, range_start)) = pending_keys.last() {
            if stack.len() < done_len {
                pending_keys.pop();
                let range_len = pos - range_start;
                let hash = range_hash(&out[range_start..pos]);
                key_cache.insert(node_key, (range_start, range_len, hash));
            } else {
                break;
            }
        }
        match node {
            RenderNode::Empty => {}
            RenderNode::Primitive(cmd) => emit_cmd!(cmd),
            RenderNode::Group { node_key, children } => {
                defer_keyed_close!(node_key);
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
            }
            RenderNode::Transform {
                node_key,
                matrix,
                children,
            } => {
                defer_keyed_close!(node_key);
                stack.push(RenderNode::Primitive(DrawCommand::PopMatrix));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushMatrix { matrix });
            }
            RenderNode::Clip {
                node_key,
                rect,
                radius,
                children,
            } => {
                defer_keyed_close!(node_key);
                stack.push(RenderNode::Primitive(DrawCommand::PopClip));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushClip { rect, radius });
            }
            RenderNode::Layer {
                node_key,
                opacity,
                backdrop_blur,
                children,
            } => {
                defer_keyed_close!(node_key);
                stack.push(RenderNode::Primitive(DrawCommand::PopLayer));
                for child in children.into_iter().rev() {
                    stack.push(child);
                }
                emit_cmd!(DrawCommand::PushLayer {
                    opacity,
                    backdrop_blur,
                });
            }
        }
    }

    // Flush any keyed closes still pending after the stack fully drained.
    for (_, node_key, range_start) in pending_keys.drain(..) {
        let range_len = pos - range_start;
        let hash = range_hash(&out[range_start..pos]);
        key_cache.insert(node_key, (range_start, range_len, hash));
    }

    // truncate stale tail entries left over from a previous longer output
    if pos != out.len() {
        out.truncate(pos);
        changed = true;
    }

    changed
}

#[cfg(test)]
mod tests {
    use geometry_core::Rect;
    use renderer_core::{Color, RectStyle};
    use rustc_hash::FxHashMap;
    use std::sync::Arc;

    use super::*;

    fn sample_rect() -> DrawCommand {
        DrawCommand::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            style: Arc::new(RectStyle::default().with_fill(Color::BLACK)),
        }
    }

    #[test]
    fn flatten_empty_returns_empty() {
        let mut out = Vec::new();
        let mut stack = Vec::new();
        let mut cache = FxHashMap::default();
        flatten_view(RenderNode::Empty, &mut out, &mut stack, &mut cache);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_group_of_empties() {
        let node = RenderNode::group([RenderNode::Empty, RenderNode::Empty, RenderNode::Empty]);
        let mut out = Vec::new();
        let mut stack = Vec::new();
        let mut cache = FxHashMap::default();
        flatten_view(node, &mut out, &mut stack, &mut cache);
        assert!(out.is_empty());
    }

    #[test]
    fn flatten_nested_groups() {
        let node = RenderNode::group([
            RenderNode::Primitive(sample_rect()),
            RenderNode::group([
                RenderNode::Primitive(sample_rect()),
                RenderNode::Empty,
                RenderNode::group([RenderNode::Primitive(sample_rect())]),
            ]),
            RenderNode::Primitive(sample_rect()),
        ]);
        let mut out = Vec::new();
        let mut stack = Vec::new();
        let mut cache = FxHashMap::default();
        flatten_view(node, &mut out, &mut stack, &mut cache);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn keyed_node_records_range_hash() {
        let node = RenderNode::group_keyed(
            42,
            [
                RenderNode::Primitive(sample_rect()),
                RenderNode::Primitive(sample_rect()),
            ],
        );
        let mut out = Vec::new();
        let mut stack = Vec::new();
        let mut cache = FxHashMap::default();
        flatten_view(node, &mut out, &mut stack, &mut cache);
        assert_eq!(out.len(), 2);
        let (start, len, _hash) = cache.get(&42).copied().expect("key 42 cached");
        assert_eq!(start, 0);
        assert_eq!(len, 2);
    }
}
