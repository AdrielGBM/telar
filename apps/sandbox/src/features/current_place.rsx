[logic]
use crate::shared::components::card::{card, CardProps};
use crate::shared::components::code_line::{code_line, CodeLineProps};
use crate::shared::components::doc_header::{doc_header, DocHeaderProps};
use crate::shared::components::example::{example, ExampleProps};

const BAR: f32 = 44.0;

let view = use_scroll_viewport();
let here = memo(move || view.as_ref().and_then(|view| view.anchor_at(BAR)));
let shown = memo(move || here.get().map_or_else(|| "nothing".to_string(), |name| name.to_string()));
let on_intro = memo(move || here.get().as_deref() == Some("place-intro"));
let on_usage = memo(move || here.get().as_deref() == Some("place-usage"));
let on_limits = memo(move || here.get().as_deref() == Some("place-limits"));
let on_page = memo(|| use_anchor_at(48.0).map_or_else(|| "none: each section here scrolls itself".to_string(), |name| name.to_string()));

[view]
col gap:20
    doc_header kicker:"NAVIGATION" title:"Current place" desc:"Which place is under a line across the view of a scroll: the anchor whose box spans that distance from the view's top edge, worked out from Telar's layout and the scroll's offset on every target. A bar over the page reads it with its own height to mark the link to the section under it with current:, and to dress itself like that section."
    example title:"anchor_at — the place under a bar, and current: on its link"
        col width:100%
            row sticky inset_top:0 height:BAR width:100% gap:8 pad_x:12 align:center fill:$theme.surface stroke:$theme.border stroke_bottom:1 role:navigation label:"Places"
                box to:anchor("place-intro") current:$on_intro pad_x:10 pad_y:4 radius:6 fill:(if $on_intro { $theme.primary } else { $theme.surface_alt })
                    text "Intro" font_size:13 color:(if $on_intro { $theme.on_primary } else { $theme.ink })
                box to:anchor("place-usage") current:$on_usage pad_x:10 pad_y:4 radius:6 fill:(if $on_usage { $theme.primary } else { $theme.surface_alt })
                    text "Usage" font_size:13 color:(if $on_usage { $theme.on_primary } else { $theme.ink })
                box to:anchor("place-limits") current:$on_limits pad_x:10 pad_y:4 radius:6 fill:(if $on_limits { $theme.primary } else { $theme.surface_alt })
                    text "Limits" font_size:13 color:(if $on_limits { $theme.on_primary } else { $theme.ink })
                text "under the bar: {$shown}" font_size:12 color:$theme.muted a11y:hidden
            col anchor:"place-intro" width:100% height:420 pad:16 gap:8 fill:$theme.surface_alt
                text "Intro" font_size:16 font_weight:600 color:$theme.ink
                text "Scroll this page: the bar sticks to the top of its view, and the link to the section under it is the current one." font_size:12 color:$theme.muted
            col anchor:"place-usage" width:100% height:420 pad:16 gap:8
                text "Usage" font_size:16 font_weight:600 color:$theme.ink
                text "A link marked current: is aria-current in a document (location, for an anchor), aria_current in AccessKit, and \"current\" in a terminal's reading." font_size:12 color:$theme.muted
            col anchor:"place-limits" width:100% height:420 pad:16 gap:8 fill:$theme.surface_alt
                text "Limits" font_size:16 font_weight:600 color:$theme.ink
                text "Where two places meet at the line, the one below it is under it; of nested places, the innermost; a place in a scroll of its own counts only where that scroll shows it." font_size:12 color:$theme.muted
        code_line code:"memo(move || view.anchor_at(44.0))   ·   box to:anchor(\"place-usage\") current:$on_usage"
    example title:"use_anchor_at — the same, on the page's own scroll"
        card gap:12
            text "Under a 48px line of the primary scroll: {$on_page}" font_size:13 color:$theme.ink
            text "A bar fixed over the page reads use_anchor_at(48.0) from anywhere on the surface, as it reads use_primary_scroll(). The sandbox scrolls each section itself, so here there is no primary scroll." font_size:12 color:$theme.muted
        code_line code:"memo(|| use_anchor_at(48.0))   ·   the anchor under a bar fixed over the page, or None"
