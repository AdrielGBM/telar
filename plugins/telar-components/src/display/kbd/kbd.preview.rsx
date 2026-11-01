[logic]
use crate::shared::{self, shown_chord};
use telar::preview::{by_role, by_text};
use telar::{ControlSize, Role};

[previews "Display/Kbd" layout:centered]

[preview "Default" matrix:themes]
kbd chord:"Mod+Shift+P" size:regular caps:KbdCaps::Each tone:ink variant:soft

[preview "Tones and variants" viewport:120x48 matrix:(mode variant tone)]
kbd chord:"Mod+D" tone:ink variant:soft

[preview "A sequence" matrix:themes]
kbd chord:"Mod+K H" caps:KbdCaps::Each

[play]
canvas.expect(by_role(Role::KeyboardInput).named(shown_chord("Mod+K H")))?;
canvas.expect_absent(by_text("K"))?;
canvas.expect_absent(by_text("H"))?;

[preview "In context" values(variant:Variant::Soft tone:Tone::Ink) matrix:themes]
col gap:(shared::gap_md(ControlSize::Regular))
    row align:center justify:between gap:(shared::gap_md(ControlSize::Regular)) pad_x:(shared::gap_md(ControlSize::Regular)) pad_y:(shared::gap_sm(ControlSize::Regular)) fill:$theme.surface color:$theme.ink radius:(shared::radius_item())
        text "Duplicate"
        kbd chord:"Mod+D" tone:tone variant:variant
    row align:center justify:between gap:(shared::gap_md(ControlSize::Regular)) pad_x:(shared::gap_md(ControlSize::Regular)) pad_y:(shared::gap_sm(ControlSize::Regular)) fill:$theme.ink color:$theme.surface radius:(shared::radius_item())
        text "Group selection"
        kbd chord:"Mod+G" tone:tone variant:variant
