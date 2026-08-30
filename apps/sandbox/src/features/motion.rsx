[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::core::theme::theme;

let big = signal(false);

// Spring-driven scale, read reactively by the declarative `box scale:$scale` below — no Canvas needed
// for a box that only scales, since `scale:` accepts a `$signal` and `Animated` is signal-backed.
let scale = motion::Animated::<f32>::new(1.0, motion::spring(170.0, 12.0));

// Six PingPong keyframe loops, each delayed a bit more by hold() so the wave enters left-to-right.
let bars: Vec<motion::Keyframes<f32>> = (0..6u64)
    .map(|i| {
        motion::Keyframes::<f32>::new(8.0)
            .hold(std::time::Duration::from_millis(i * 110))
            .then(
                48.0,
                std::time::Duration::from_millis(300),
                motion::Easing::EaseInOut,
            )
            .start(motion::Repeat::PingPong)
    })
    .collect();
let equalizer = move |rect: Rect| {
    let t = theme();
    let palette = [t.primary, t.success, t.warning, t.danger, t.purple, t.cyan];
    let (bar_w, gap) = (24.0f32, 8.0f32);
    let baseline = rect.y + rect.height;
    let render: Vec<RenderNode> = bars
        .iter()
        .enumerate()
        .map(|(i, kf)| {
            let h = kf.get();
            RenderNode::rect(
                Rect {
                    x: rect.x + i as f32 * (bar_w + gap),
                    y: baseline - h,
                    width: bar_w,
                    height: h,
                },
                RectStyle::filled(palette[i % palette.len()], 4.0),
            )
        })
        .collect();
    RenderNode::group(render)
};

// A one-shot timeline; the button restarts this same handle.
let progress = motion::Keyframes::<f32>::new(0.0)
    .then(
        100.0,
        std::time::Duration::from_millis(1100),
        motion::Easing::EaseInOut,
    )
    .start(motion::Repeat::Once);
let progress_canvas = progress.clone();
let progress_bar = move |rect: Rect| {
    let t = theme();
    let pct = (progress_canvas.get() / 100.0).clamp(0.0, 1.0);
    RenderNode::group([
        RenderNode::rect(
            rect,
            RectStyle::filled(Color::rgba(t.muted.r, t.muted.g, t.muted.b, 0.25), 7.0),
        ),
        RenderNode::rect(
            Rect {
                x: rect.x,
                y: rect.y,
                width: rect.width * pct,
                height: rect.height,
            },
            RectStyle::filled(t.primary, 7.0),
        ),
    ])
};

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Motion" desc:"Beyond transitions, the motion kernel gives you springs and keyframe timelines driven from Rust — velocity-preserving bounces, staggered loops, and one-shot playback."
    example title:"Spring — retarget a value and it settles with a natural bounce"
        card gap:12
            row gap:20 align:center
                box width:100 height:100 align:center justify:center
                    box fill:theme.primary radius:12 width:60 height:60 scale:$scale
                button label:"Bounce" fill:theme.primary on_press:(|| { $big.toggle(); $scale.retarget(if $big.get() { 1.3 } else { 0.6 }) })
        code_line code:"box fill:theme.primary radius:12 width:60 height:60 scale:$scale   // scale.retarget(1.3)"
    example title:"Staggered keyframes — six PingPong loops offset by hold()"
        card
            canvas paint:equalizer width:196 height:56
        code_line code:"Keyframes::new(8.0).hold(i·110ms).then(48.0, 300ms, EaseInOut).start(Repeat::PingPong)"
    example title:"One-shot timeline — Replay restarts the same handle"
        card gap:12
            row gap:12 align:center
                canvas paint:progress_bar width:240 height:14
                text "{$progress.round()}%" font_size:12 color:theme.muted
                button label:"Replay" fill:theme.primary on_press:(|| { $progress.restart() })
        code_line code:"Keyframes::new(0.0).then(100.0, 1100ms, EaseInOut).start(Repeat::Once)"
