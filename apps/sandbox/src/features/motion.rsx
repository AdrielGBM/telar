[logic]
use crate::core::theme::theme;

let big = signal(false);

// Spring-driven scale. The canvas gets its own clone so `scale` stays free for the button's retarget.
let scale = motion::Animated::<f32>::new(1.0, motion::spring(170.0, 12.0));
let scale_canvas = scale.clone();
let spring_box = Canvas::new(
    ctx,
    LayoutStyle::new().width(100.0).height(100.0),
    move |rect| {
        let s = scale_canvas.get();
        let cx = rect.x + 50.0;
        let cy = rect.y + 50.0;
        let m = Transform::scale_around(s, s, cx, cy).to_array();
        RenderNode::transform_with(
            m,
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
                    radius: BorderRadius::all(12.0),
                },
            )],
        )
    },
)?;

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
let equalizer = Canvas::new(
    ctx,
    LayoutStyle::new().width(196.0).height(56.0),
    move |rect| {
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
                    RectStyle {
                        fill: Some(Paint::Solid(palette[i % palette.len()])),
                        stroke: None,
                        shadow: None,
                        radius: BorderRadius::all(4.0),
                    },
                )
            })
            .collect();
        RenderNode::group(render)
    },
)?;

// A one-shot timeline; the button restarts this same handle.
let progress = motion::Keyframes::<f32>::new(0.0)
    .then(
        100.0,
        std::time::Duration::from_millis(1100),
        motion::Easing::EaseInOut,
    )
    .start(motion::Repeat::Once);
let progress_canvas = progress.clone();
let progress_bar = Canvas::new(
    ctx,
    LayoutStyle::new().width(240.0).height(14.0),
    move |rect| {
        let t = theme();
        let pct = (progress_canvas.get() / 100.0).clamp(0.0, 1.0);
        RenderNode::group([
            RenderNode::rect(
                rect,
                RectStyle {
                    fill: Some(Paint::Solid(Color::rgba(
                        t.muted.r, t.muted.g, t.muted.b, 0.25,
                    ))),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(7.0),
                },
            ),
            RenderNode::rect(
                Rect {
                    x: rect.x,
                    y: rect.y,
                    width: rect.width * pct,
                    height: rect.height,
                },
                RectStyle {
                    fill: Some(Paint::Solid(t.primary)),
                    stroke: None,
                    shadow: None,
                    radius: BorderRadius::all(7.0),
                },
            ),
        ])
    },
)?;

[view]
col gap:20
    doc_header kicker:"16 · INTERACTION" title:"Motion" desc:"Beyond transitions, the motion kernel gives you springs and keyframe timelines driven from Rust — velocity-preserving bounces, staggered loops, and one-shot playback."
    col gap:8
        text "Spring — retarget a value and it settles with a natural bounce" size:13 color:ink
        card gap:12
            row gap:20 align:center
                widget "spring_box"
                button label:"Bounce" fill:primary on_press:|| { let b = $big.peek(); $big.set(!b); $scale.retarget(if b { 0.6 } else { 1.3 }) }
        code_line code:"let scale = motion::Animated::new(1.0, spring(170, 12));   scale.retarget(1.3)"
    col gap:8
        text "Staggered keyframes — six PingPong loops offset by hold()" size:13 color:ink
        card
            widget "equalizer"
        code_line code:"Keyframes::new(8.0).hold(i·110ms).then(48.0, 300ms, EaseInOut).start(Repeat::PingPong)"
    col gap:8
        text "One-shot timeline — Replay restarts the same handle" size:13 color:ink
        card gap:12
            row gap:12 align:center
                widget "progress_bar"
                text "{$progress.round()}%" size:12 color:muted
                button label:"Replay" fill:primary on_press:|| { $progress.restart() }
        code_line code:"Keyframes::new(0.0).then(100.0, 1100ms, EaseInOut).start(Repeat::Once)"
