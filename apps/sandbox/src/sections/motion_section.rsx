[logic]
use crate::theme::theme;

let fade = signal(1.0f32);
let scale = motion::Animated::<f32>::new(1.0, motion::spring(170.0, 12.0));

// The Canvas closure needs its own handle so `scale` survives, uncaptured, for the toggle button's `$scale.retarget(..)` below.
let scale_canvas = scale.clone();
// Fixed 100x100 slot: the spring-scaled rect is painted via a transform, which layout cannot see, so the canvas must reserve the space itself or the square draws over its row neighbors.
let result = Canvas::new(ctx, LayoutStyle::new().width(100.0).height(100.0), move |rect| {
    let s = scale_canvas.get();
    let cx = rect.x + 50.0;
    let cy = rect.y + 50.0;
    let matrix = Transform::scale_around(s, s, cx, cy).to_array();
    RenderNode::transform_with(
        matrix,
        [RenderNode::rect(
            Rect {
                x: cx - 30.0,
                y: cy - 30.0,
                width: 60.0,
                height: 60.0,
            },
            RectStyle {
                fill: Some(Paint::Solid(theme().primary)),
                stroke: None,
                shadow: None,
                radius: BorderRadius::all(10.0),
            },
        )],
    )
})?;

[view]
col gap:8
    text "Motion" size:12 color:muted
    text "Toggle dims the left box with a 300ms opacity tween and bounces the right one with a velocity-preserving spring" size:11 color:muted
    row gap:24 align:center
        col gap:4 align:center
            box width:64 height:64 fill:primary radius:10 opacity:$fade transition:opacity 300ms ease-in-out
            text "opacity tween" size:11 color:muted
        col gap:4 align:center
            widget "result"
            text "scale spring" size:11 color:muted
        btn "Toggle" fill:primary on_press:|| { let dim = $fade.peek() > 0.5; $fade.set(if dim { 0.15 } else { 1.0 }); $scale.retarget(if dim { 0.6 } else { 1.25 }) }

[preview "Motion"]
sections_motion_section
