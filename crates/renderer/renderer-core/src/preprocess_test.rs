use super::*;
use crate::{BorderRadius, RectStyle};
use geometry_core::Rect;

fn rect_cmd(style: &Arc<RectStyle>, x: f32) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(x, 0.0, 10.0, 10.0),
        style: style.clone(),
    }
}

#[test]
fn scale_into_correctly_scales_and_shares_arcs() {
    let style = Arc::new(RectStyle::default().with_radius(BorderRadius::all(4.0)));
    let cmds = vec![
        rect_cmd(&style, 0.0),
        rect_cmd(&style, 20.0),
        rect_cmd(&style, 40.0),
    ];
    let sf = 3.0;

    let mut scratch = ScaleScratch::new();
    let got = scratch.scale_into(&cmds, sf);

    assert_eq!(got.len(), 3);

    for (i, cmd) in got.iter().enumerate() {
        match cmd {
            DrawCommand::Rect { rect, style: _ } => {
                assert_eq!(rect.x, (i as f32) * 20.0 * sf);
                assert_eq!(rect.y, 0.0);
                assert_eq!(rect.width, 30.0);
                assert_eq!(rect.height, 30.0);
            }
            other => panic!("expected Rect, got {other:?}"),
        }
    }

    // The three commands shared one input style Arc, so the scaled output Arcs are shared too (one Arc::new instead of three).
    let style_of = |c: &DrawCommand| match c {
        DrawCommand::Rect { style, .. } => style.clone(),
        _ => unreachable!(),
    };
    let a0 = style_of(&got[0]);
    let a1 = style_of(&got[1]);
    let a2 = style_of(&got[2]);
    assert!(
        Arc::ptr_eq(&a0, &a1),
        "an unscaled Arc is shared, not cloned"
    );
    assert!(Arc::ptr_eq(&a1, &a2), "across every command that names it");
    assert_eq!(a0.radius.top_left, 12.0);
}

#[test]
fn scale_into_reuses_buffer_across_frames() {
    let style = Arc::new(RectStyle::default());
    let cmds = vec![rect_cmd(&style, 0.0), rect_cmd(&style, 10.0)];
    let mut scratch = ScaleScratch::new();
    let _ = scratch.scale_into(&cmds, 2.0);
    let cap = scratch.storage.capacity();
    assert!(
        cap >= cmds.len(),
        "the buffer is reused, so it keeps the capacity it earned: {cap}"
    );
    let _ = scratch.scale_into(&cmds, 2.0);
    // Buffer capacity persists between frames: no per-frame Vec reallocation for the same command count.
    assert_eq!(scratch.storage.capacity(), cap);
}
