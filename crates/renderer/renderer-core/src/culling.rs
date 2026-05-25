use geometry_core::Rect;

use crate::DrawCommand;

fn union_rects(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    Rect::new(x, y, right - x, bottom - y)
}

pub fn overlaps(x: f32, y: f32, w: f32, h: f32, clip: Option<Rect>) -> bool {
    match clip {
        None => true,
        Some(c) => x < c.x + c.width && x + w > c.x && y < c.y + c.height && y + h > c.y,
    }
}

pub fn expand_for_shadow(
    rect: Rect,
    blur_radius: f32,
    spread: f32,
    offset_x: f32,
    offset_y: f32,
) -> Rect {
    let expand = blur_radius + spread;
    let expanded = Rect::new(
        rect.x - expand,
        rect.y - expand,
        rect.width + expand * 2.0,
        rect.height + expand * 2.0,
    );
    let shifted = Rect::new(
        expanded.x + offset_x,
        expanded.y + offset_y,
        expanded.width,
        expanded.height,
    );
    union_rects(rect, shifted)
}

pub fn command_visual_rect(cmd: &DrawCommand, cum_tx: f32, cum_ty: f32) -> Option<Rect> {
    match cmd {
        DrawCommand::Rect(p) => {
            let r = Rect::new(
                p.rect.x + cum_tx,
                p.rect.y + cum_ty,
                p.rect.width,
                p.rect.height,
            );
            Some(match p.style.shadow {
                Some(s) => expand_for_shadow(r, s.blur_radius, s.spread, s.offset_x, s.offset_y),
                None => r,
            })
        }
        DrawCommand::Text(p) => {
            let r = Rect::new(
                p.rect.x + cum_tx,
                p.rect.y + cum_ty,
                p.rect.width,
                p.rect.height,
            );
            Some(match p.style.shadow {
                Some(s) => expand_for_shadow(r, s.blur_radius, s.spread, s.offset_x, s.offset_y),
                None => r,
            })
        }
        DrawCommand::Image { rect, .. } => Some(Rect::new(
            rect.x + cum_tx,
            rect.y + cum_ty,
            rect.width,
            rect.height,
        )),
        DrawCommand::Line { p1, p2, style } => {
            let half_w = style.width / 2.0;
            let x = p1.x.min(p2.x) + cum_tx - half_w;
            let y = p1.y.min(p2.y) + cum_ty - half_w;
            let right = p1.x.max(p2.x) + cum_tx + half_w;
            let bottom = p1.y.max(p2.y) + cum_ty + half_w;
            Some(Rect::new(x, y, right - x, bottom - y))
        }
        DrawCommand::Path(p) => {
            let base = p.data.bounds()?;
            let r = Rect::new(base.x + cum_tx, base.y + cum_ty, base.width, base.height);
            Some(match p.style.shadow {
                Some(s) => expand_for_shadow(r, s.blur_radius, s.spread, s.offset_x, s.offset_y),
                None => r,
            })
        }
        DrawCommand::PushClip { .. }
        | DrawCommand::PopClip
        | DrawCommand::PushTransform { .. }
        | DrawCommand::PopTransform
        | DrawCommand::PushLayer { .. }
        | DrawCommand::PopLayer => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlaps_no_clip() {
        assert!(overlaps(0.0, 0.0, 10.0, 10.0, None));
    }

    #[test]
    fn overlaps_inside_clip() {
        let clip = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(overlaps(10.0, 10.0, 20.0, 20.0, Some(clip)));
    }

    #[test]
    fn overlaps_outside_clip() {
        let clip = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(!overlaps(20.0, 20.0, 5.0, 5.0, Some(clip)));
    }

    #[test]
    fn expand_for_shadow_expands_all_sides() {
        let r = Rect::new(10.0, 10.0, 20.0, 20.0);
        let result = expand_for_shadow(r, 5.0, 2.0, 0.0, 0.0);
        assert!(result.x < r.x);
        assert!(result.y < r.y);
        assert!(result.x + result.width > r.x + r.width);
        assert!(result.y + result.height > r.y + r.height);
    }
}
