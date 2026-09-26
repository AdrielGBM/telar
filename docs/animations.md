# Animations

How a value in telar changes over time, and where the framework refuses to let it.

This page is referenced from the code — `telar/src/lib.rs` cites **D2**, `telar-transpiler/src/transition.rs`
cites **F5** and the `.rsx` syntax below. Those citations outlived the document itself for a while; the
identifiers here are the ones they name.

## Design

### D1. One ticker, not one per animation

Every in-flight animation advances from a single frame ticker in `motion-core`. A widget does not own a timer,
and nothing schedules a wake-up per value. What an animation costs is therefore a function of how many frames
are drawn, not of how many things are moving.

The consequence is a rule the runner must hold to: **an animation in flight need not dirty the tree.** A spring
approaching its target produces values that round to the same pixels, so a frame budget that paces off "when did
content last change" starves the animation and then never recovers. The tick clock advances every turn of the
loop for this reason.

### D2. `motion` is always on

`telar` re-exports `motion_core` with no feature gate. The transpiler emits `motion::Animated` /
`motion::tween` / `motion::spring` / `motion::Easing` paths against that re-export, so gating it would make
`transition(…)` compile or not depending on a feature the author never mentions. It is kernel functionality, not
an opt-in module.

### D3. Colours interpolate in Oklch

A fade between two colours goes through Oklch, not sRGB, so a blue-to-yellow transition does not pass through
grey. `Lerp` is implemented per type; anything that can name a midpoint can be animated.

### D4. `Timeline<T>` is progress-driven; `Keyframes<T>` is time-driven on top of it

`motion::Timeline<T: Lerp>` is a sequence of legs (`TimelineBuilder::then`/`hold`, same shape as
`Keyframes`) sampled by `sample(p)` for a progress `p ∈ [0, 1]` — `p=0` is the first step's start, `p=1`
the last step's end, and the sampling itself is the same `value_at` interpolation `Keyframes` uses per
frame. A `Timeline` owns no clock and registers with nothing: it is what something that already has a
progress value — a scroll or view range, a drag gesture, a scrubber — samples directly.

`Keyframes<T>` is a `Timeline<T>` driven by the ticker: it converts elapsed wall-clock time into a
progress fraction of the sequence's total duration and samples the same machinery every frame. The two
are not a parallel pair of interpolators; `KeyframesBuilder` builds a `Timeline` internally and
`Keyframes` never re-implements `value_at`.

```rust
let tl = motion::Timeline::builder(0.0f32)
    .then(1.0, Duration::from_millis(200), motion::Easing::EaseInOut)
    .build();
tl.sample(0.5); // the value halfway through the sequence, no clock involved
```

### D5. One time scale, and reduced motion zeroes it

`motion::set_scale` is the global time scale every registered animation advances by: `1.0` is normal, values in
between are slow motion, and `0.0` makes every animation jump straight to its end. A looping `Keyframes` jumps to
the end of the step in flight instead of freezing.

When the user asks their system for less motion (`use_reduced_motion() == Some(true)`, see
[system-preferences.md](system-preferences.md)), the ticker hands every animation a scale of `0.0` without
touching the one the application set, so turning the preference off restores it. It is read on every tick, so
the change applies from the next frame on every target that reports the preference. Where the preference is
unknown (a terminal, a headless run that did not declare it) nothing changes.

Two things are exempt:

- **Momentum.** A scroll that carries on after the finger lifts is the content following the hand, and every
  platform keeps it when motion is reduced. It answers `false` to `Tickable::reducible`. A wheel notch's easing
  is an animation and does jump.
- **An application that opts out.** `motion::follow_reduced_motion(false)` stops the ticker applying the
  preference. Use it only when the application tones its motion down itself, by reading `use_reduced_motion()`
  and choosing gentler animations; an application that simply prefers its animations is overriding the user.

The switch belongs to each runtime: a hot-reloaded library or a plugin reads its own copy of the preferences and
keeps its own switch.

## The `.rsx` transition syntax

```
transition(<prop> <duration> [<easing> | spring(<stiffness>, <damping>)])
```

- `<duration>` is `200ms` or `0.3s`.
- `<easing>` is `linear`, `ease-in`, `ease-out`, `ease-in-out`, `cubic-bezier(a,b,c,d)`, or
  `steps(n[, position])`; it defaults to `ease-out`.
- `steps(n[, position])` holds `n` flat plateaus instead of a continuous curve, with CSS-equivalent
  `<step-position>` semantics. `position` is `jump-start`, `jump-end`, `jump-none`, or `jump-both` (the
  legacy CSS aliases `start`/`end` also work), and defaults to `jump-end` when omitted — same as CSS
  `steps(n)`. `jump-start` steps immediately at `p=0`; `jump-end` holds at `0` until the first interval
  completes; `jump-none` holds flat at both `p=0` and `p=1` (needs `n >= 2`); `jump-both` adds a plateau
  at each end. It compiles to `motion::Easing::Steps(n, motion::StepPosition::…)`.
- A `spring(k, c)` replaces the duration and easing entirely.
- Several properties separate with commas: `transition(opacity 200ms, fill 100ms linear)`.
- The parentheses are the value's delimiter, not decoration: they are what lets a value hold a space, so the
  attribute reads whole and sits anywhere on its line.

Each property named must have a matching value on the same element — `transition(fill 200ms)` with no `fill:`
is an error, not a no-op, because the alternative is an animation that silently animates nothing.

### F5. What can be animated, and why the line is there

**Paint** — `opacity`, `fill`, `stroke`, `color`.
**Transform** — `rotate`, `scale`, `scale_x`, `scale_y`, `translate_x`, `translate_y`.

Both halves are read per frame from a closure the renderer already re-runs, so animating them costs a repaint
and nothing else.

**The layout box is deliberately out.** Animating `width`/`height`/`x`/`y` would put a layout pass in every
frame of every transition — a different order of cost, and a decision that needs its own argument rather than
arriving as a side effect of this one. Anything else named in a `transition(…)` is a `compile_error!` that names
the property, because a silently-ignored animation looks exactly like a broken one.

**Out of a transition is not fixed forever.** A layout style *can* be re-resolved from reactive state:
`ui_core::style_follows` / `StyledContainer::styled_by` rebuild one under an effect, which is how the
catalogue's spacing follows a live theme switch. The line is *per frame* against *per change* — a theme switch
is a handful of relayouts when the user does something, a transition is one on every frame for its whole
duration. Same machinery, two orders of cost, and only one of them belongs behind `transition(…)`. A
[breakpoint](surface-size.md#breakpoints) is the same machinery again: a `pad:$gutter` read from a followed
breakpoint re-resolves once per threshold crossed, not once per pixel of a resize.

A moving indicator does not need the layout box. `track_rect:` reads where a node was laid out, and
`translate_x` moves a sibling to it:

```rsx
[logic]
let active = signal(Rect::new(0.0, 0.0, 0.0, 0.0));
let at = active.read_only();

[view]
row
    box track_rect:$active width:40 height:4
    box width:40 height:4 translate_x:at.get().x transition(translate_x 200ms)
```

### F7. An `Animated` persists across re-renders

A `transition(…)` hoists its `Animated` into the component's setup scope, not into the view closure. The handle is
built once per component instance and survives every `view()` re-run, which is what makes a transition continue
rather than restart on each frame. `retarget` is a no-op when the target has not moved.

Two consequences worth knowing:

- **A settled `Animated` schedules no frames.** One constructed at its destination is inert — build it away from
  the goal and retarget at once.
- **The settle epsilons are absolute**, so a spring on a value whose natural scale is large (screen coordinates,
  say) needs its own tuning rather than the defaults.

## Still open

- **Declarative keyframes.** Today a multi-stage animation is the Rust API; there is no `.rsx` spelling, and how
  it would compose with the existing single-property `transition(…)` is unresolved.
- **A changed `PushLayer` forces a full software repaint**, so an animated layer costs more on the CPU backend
  than the same animation without one.
