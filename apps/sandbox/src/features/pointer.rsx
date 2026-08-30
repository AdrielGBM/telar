[logic]
// The marker is positioned from this, so neither box has to know the other's size — a sliding indicator.
let handle = signal(Rect::default());
let marker_x = memo(move || handle.get().x);
let marker_w = memo(move || handle.get().width);
let tab = signal(0i32);

let at_x = signal(0.0f32);
let at_y = signal(0.0f32);

let settled = signal("nothing yet".to_string());
let wheel = signal(0.0f32);
let alt = signal("no alt-click yet".to_string());

[view]
col gap:20
    doc_header kicker:"INTERACTION" title:"Pointer & drag" desc:"The pointer attributes a box can carry: which cursor it claims, which buttons start a drag, and the callbacks for moving, finishing, scrolling and alt-clicking. track_rect mirrors a laid-out rect back into a signal, which is how one box follows another."
    example title:"cursor — what the pointer becomes over this box"
        card gap:12
            row gap:10 wrap
                box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:14 pad_y:10 cursor:pointer
                    text "pointer" font_size:12 color:theme.ink
                box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:14 pad_y:10 cursor:crosshair
                    text "crosshair" font_size:12 color:theme.ink
                box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:14 pad_y:10 cursor:grab
                    text "grab" font_size:12 color:theme.ink
                box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:14 pad_y:10 cursor:col_resize
                    text "col_resize" font_size:12 color:theme.ink
                box fill:theme.surface_alt stroke:theme.border radius:8 pad_x:14 pad_y:10 cursor:not_allowed
                    text "not_allowed" font_size:12 color:theme.ink
        code_line code:"box cursor:pointer   ·   crosshair · grab · grabbing · col_resize · row_resize · text · wait · not_allowed"
    example title:"track_rect — mirror a laid-out rect into a signal, and position a sibling from it"
        card gap:12
            col gap:8 width:100%
                row gap:8
                    box fill:theme.surface_alt radius:8 pad_x:16 pad_y:8 cursor:pointer track_rect:$handle on_press(|| $tab.set(0))
                        text "Overview" font_size:12 color:theme.ink
                    box fill:theme.surface_alt radius:8 pad_x:16 pad_y:8 cursor:pointer on_press(|| $tab.set(1))
                        text "Details" font_size:12 color:theme.ink
                box height:3 width:100%
                    box fill:theme.primary radius:2 height:3 absolute inset_start:$marker_x width:$marker_w
            text "The underline is sized and placed from the first tab's own rect — no measurement in [logic]." font_size:12 color:theme.muted
        code_line code:"box track_rect:$handle …      then      box absolute inset_start:$x width:$w"
    example title:"on_pointer_move — every move over the box, in its own coordinates"
        card gap:10
            box fill:theme.surface_alt stroke:theme.border radius:10 height:96 width:100% cursor:crosshair on_pointer_move(|x, y| { $at_x.set(x); $at_y.set(y) })
            text "x {$at_x.round()} · y {$at_y.round()}" font_size:13 color:theme.primary
        code_line code:"box on_pointer_move:|x, y| { $at_x.set(x); $at_y.set(y) }"
    example title:"on_drag_end — the gesture is over, and this is where it stopped"
        card gap:10
            box fill:theme.surface_alt stroke:theme.border radius:10 height:80 width:100% cursor:grab active_style(fill:theme.primary) on_drag(|x, _y| $at_x.set(x)) on_drag_end(|x, y| $settled.set(format!("{x:.0}, {y:.0}")))
            text "settled at {$settled}" font_size:13 color:theme.muted
        code_line code:"box on_drag:|x, _| …  on_drag_end:|x, y| …    ·   active_style(fill:…) while the press is held"
    example title:"drag_button — arm a drag on buttons other than the primary one"
        card gap:10
            box fill:theme.surface_alt stroke:theme.border radius:10 height:72 width:100% cursor:grab drag_button:secondary,auxiliary on_drag(|x, _y| $at_x.set(x))
                text "drag me with the right or middle button too" font_size:12 color:theme.muted pad:12
        code_line code:"box drag_button:secondary,auxiliary     (the primary button is always armed)"
    example title:"on_scroll and on_alt_press — the wheel, and a non-primary click"
        card gap:10
            box fill:theme.surface_alt stroke:theme.border radius:10 height:80 width:100% on_scroll(|_dx, dy| $wheel.set($wheel.get() + dy)) on_alt_press(|button| $alt.set(format!("{button:?}")))
                text "scroll over me, or click with the right button" font_size:12 color:theme.muted pad:12
            row gap:16
                text "wheel {$wheel.round()}" font_size:13 color:theme.primary
                text "{$alt}" font_size:13 color:theme.muted
        code_line code:"box on_scroll:|dx, dy| …    ·    on_alt_press:|button| …"
    example title:"Attributes"
        col gap:6
            prop_row name:"cursor" values:"pointer · crosshair · grab · …" about:"What the pointer becomes while it is over this box."
            prop_row name:"track_rect" values:"$signal" about:"Mirrors this box's laid-out rect into the signal, every layout pass."
            prop_row name:"drag_button" values:"secondary · auxiliary" about:"Extra buttons that may start this box's drag."
            prop_row name:"on_drag / on_drag_end" values:"closure (x, y)" about:"The pointer during the gesture, and where it finished."
            prop_row name:"on_pointer_move" values:"closure (x, y)" about:"Every move over the box, whether or not a button is down."
            prop_row name:"on_scroll" values:"closure (dx, dy)" about:"Wheel or trackpad deltas over the box."
            prop_row name:"on_alt_press" values:"closure (button)" about:"A press from a non-primary button."
            prop_row name:"active_style" values:"fill: · stroke: · radius:" about:"Paint applied while the box is held down, like hover_style for hover."
