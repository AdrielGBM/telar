[logic]
use crate::theme::theme;

// Every box in the loop below reads this same signal, so toggling it once cross-fades all of them — each through its own per-item `Animated` hoisted inside the `for` body.
let accent = signal(theme().primary);
let accent_alt = signal(false);

// Each bar's `hold` delays its bounce by a bit more than the last, so the wave visibly enters left-to-right instead of every bar moving in lockstep.
let bar_count = 6u64;
let bars: Vec<motion::Keyframes<f32>> = (0..bar_count)
    .map(|i| {
        motion::Keyframes::<f32>::new(10.0)
            .hold(std::time::Duration::from_millis(i * 120))
            .then(46.0, std::time::Duration::from_millis(320), motion::Easing::EaseInOut)
            .start(motion::Repeat::PingPong)
    })
    .collect();
let equalizer = Canvas::new(ctx, LayoutStyle::new().width(184.0).height(56.0), move |rect| {
    let t = theme();
    let palette = [t.primary, t.success, t.warning, t.danger, t.purple, t.cyan];
    let bar_w = 24.0f32;
    let gap = 8.0f32;
    let baseline = rect.y + rect.height;
    let bars_render: Vec<RenderNode> = bars
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
    RenderNode::group(bars_render)
})?;

// A `Repeat::Once` sequence plays exactly once; the canvas gets its own clone so `progress` stays free, uncaptured, for the Replay button's `$progress.restart()` below.
let progress = motion::Keyframes::<f32>::new(0.0)
    .then(100.0, std::time::Duration::from_millis(1000), motion::Easing::EaseInOut)
    .start(motion::Repeat::Once);
let progress_canvas = progress.clone();
let progress_bar = Canvas::new(ctx, LayoutStyle::new().width(220.0).height(16.0), move |rect| {
    let t = theme();
    let pct = (progress_canvas.get() / 100.0).clamp(0.0, 1.0);
    RenderNode::group([
        RenderNode::rect(
            rect,
            RectStyle {
                fill: Some(Paint::Solid(Color::rgba(t.muted.r, t.muted.g, t.muted.b, 0.25))),
                stroke: None,
                shadow: None,
                radius: BorderRadius::all(8.0),
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
                radius: BorderRadius::all(8.0),
            },
        ),
    ])
})?;

[view]
col gap:16
    text "Motion Lab" size:12 color:muted
    text "Loop transitions, staggered keyframes, and a restartable one-shot animation" size:11 color:muted
    col gap:8
        text "A for-loop of boxes, each fill:$accent transition:fill — one persistent Animated per item, all driven by one shared signal" size:11 color:muted
        row gap:12 align:center
            for label in ["A", "B", "C", "D", "E"].into_iter()
                col gap:4 align:center
                    box width:48 height:48 fill:$accent radius:10 transition:fill 250ms ease-out
                    text "Chip {label}" size:10 color:muted
        btn "Toggle accent" fill:primary on_press:|| { let on = $accent_alt.peek(); $accent_alt.set(!on); $accent.set(if on { theme().primary } else { theme().purple }) }
    col gap:8
        text "Six Keyframes<f32> PingPong loops, each staggered by hold() — a mini equalizer" size:11 color:muted
        widget "equalizer"
    col gap:8
        text "A Repeat::Once keyframes sequence; Replay calls restart() on the same handle" size:11 color:muted
        row gap:12 align:center
            widget "progress_bar"
            text "{$progress.round()}%" size:11 color:muted
            btn "Replay" fill:primary on_press:|| { $progress.restart() }

[preview "Motion Lab"]
sections_motion_lab_section
