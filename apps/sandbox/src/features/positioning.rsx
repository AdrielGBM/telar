[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};
use crate::shared::components::prop_row::{prop_row, PropRowProps};
// Which corner the pinned badge sits in. The badge never moves in the tree — only which insets name it does.
let corner = signal(0i32);
let badge_top = memo(move || if corner.get() < 2 { 12.0 } else { 76.0 });
let badge_start = memo(move || if corner.get() % 2 == 0 { 12.0 } else { 140.0 });

// The scrutinee of the reactive `match` below. `Hash` and `Eq` are what let an arm be keyed on its payload.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Slot {
    Empty,
    Filled(u8),
}
let slot = signal(Slot::Empty);

// Matched once at construction: no `$` in the scrutinee, so this is an ordinary Rust `match`.
let caption: Option<&'static str> = Some("read from a plain Option, decided once");

[view]
col gap:20
    doc_header kicker:"FOUNDATIONS" title:"Positioning" desc:"absolute takes a child out of the flex flow and pins it by the insets it names. Insets and margins are logical — start and end follow the reading direction, so a layout mirrors under RTL without a second rule."
    example title:"absolute — a child out of flow, pinned by the edges it names"
        card gap:12
            box fill:$theme.surface_alt radius:10 height:120 width:100% pad:14
                text "still in the flow" font_size:12 color:$theme.muted
                box fill:$theme.primary radius:6 width:60 height:24 absolute inset_top:12 inset_end:12
                box fill:$theme.purple radius:6 width:60 height:24 absolute inset_bottom:12 inset_start:12
        code_line code:"box absolute inset_top:12 inset_end:12     ·   inset_bottom · inset_start"
    example title:"absolute:fill — the all-four-edges-at-zero shorthand"
        card gap:12
            box fill:$theme.surface_alt radius:10 height:96 width:100% pad:14
                text "under the scrim" font_size:13 color:$theme.ink
                box fill:$theme.primary radius:10 absolute:fill opacity:0.35
        code_line code:"box absolute:fill      (what overlay uses; name three edges instead for a floating panel)"
    example title:"An inset is an ordinary layout value, so it can be reactive"
        card gap:12
            box fill:$theme.surface_alt radius:10 height:120 width:220
                box fill:$theme.primary radius:6 width:60 height:24 absolute inset_top:$badge_top inset_start:$badge_start
            row gap:10
                button label:"Move the badge" ghost on_press:(|| $corner.set(($corner.get() + 1) % 4))
        code_line code:"box absolute inset_top:$top inset_start:$start     (the node keeps an effect and re-styles)"
    example title:"margin_start / margin_end — logical margins that mirror with the reading direction"
        card gap:10
            col gap:8 width:100%
                box fill:$theme.border radius:6 height:22
                box fill:$theme.cyan radius:6 height:22 margin_start:40
                box fill:$theme.cyan radius:6 height:22 margin_end:40
            text "The first bar is the full width; the two below give up 40 at the start and at the end." font_size:12 color:$theme.muted
        code_line code:"box margin_start:40      (left under LTR, right under RTL — switch the locale to see it)"
    example title:"min_height — a floor a box never collapses below"
        card gap:10
            row gap:10 align:start
                box fill:$theme.surface_alt stroke:$theme.border radius:8 min_height:72 width:130 pad:10
                    text "short" font_size:12 color:$theme.muted
                box fill:$theme.surface_alt stroke:$theme.border radius:8 min_height:72 width:130 pad:10
                    text "long enough to push past the floor on its own" font_size:12 color:$theme.muted
        code_line code:"box min_height:72        ·   min_width · max_width · max_height"
    example title:"text_wrap — refuse the line break and let the box clip instead"
        card gap:10
            col gap:8 width:270
                text "This sentence wraps at the column edge, which is the default." font_size:13 color:$theme.ink
                box fill:$theme.surface_alt radius:8 pad:8 clip
                    text "This one keeps going instead of wrapping." font_size:13 color:$theme.ink text_wrap:nowrap
        code_line code:"text text_wrap:nowrap    ·   wrap (the default)"
    example title:"match — choose a subtree by variant, once or on every change"
        card gap:12
            match caption
                Some(note)
                    text "{note}" font_size:12 color:$theme.muted
                None
                    text "no caption" font_size:12 color:$theme.muted
            match $slot as s key *s
                Slot::Empty
                    box fill:$theme.surface_alt stroke:$theme.border radius:8 width:150 height:44 align:center justify:center
                        text "empty" font_size:12 color:$theme.muted
                Slot::Filled(n)
                    box fill:$theme.success radius:8 width:150 height:44 align:center justify:center
                        text "tile {n}" font_size:12 color:$theme.on_primary
            row gap:10
                button label:"Fill" fill:$theme.primary on_press:(|| $slot.set(Slot::Filled(7)))
                button label:"Clear" ghost on_press:(|| $slot.set(Slot::Empty))
        code_line code:"match caption > Some(note) > text …            (no $ — the arm is chosen once)"
        code_line code:"match $slot as s key *s > Slot::Empty > box …  (reactive; the key decides when an arm rebuilds)"
    example title:"Attributes"
        col gap:6
            prop_row name:"absolute" values:"flag · fill" about:"Out of flow. Bare pins only the edges you name; fill pins all four at zero."
            prop_row name:"inset_start / _end" values:"number · %" about:"Distance from the leading and trailing edge — mirrors under RTL."
            prop_row name:"inset_top / _bottom" values:"number · %" about:"Distance from the top and bottom edge."
            prop_row name:"margin_start / _end" values:"number · %" about:"Logical outer spacing, in the flow rather than out of it."
            prop_row name:"min_height" values:"number · %" about:"A floor the box keeps even when its content is smaller."
            prop_row name:"text_wrap" values:"wrap · nowrap" about:"Whether a line may break at the box edge."
