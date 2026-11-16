[logic]
use crate::shared;
use telar::preview::by_role;
use telar::{ControlSize, Role};

[previews "Feedback/Spinner" layout:centered]
Use `progress` when the work can say how far along it is, a spinner when it cannot.

[preview "Default" matrix:themes]
spinner size:regular tone:ink a11y_label:"Loading"

[play]
canvas.expect(by_role(Role::ProgressBar).named("Loading"))?;

[preview "Tones and sizes" viewport:120x48 matrix:(mode size tone)]
spinner size:regular tone:ink a11y_label:"Loading"

[preview "In context" matrix:themes]
col gap:(shared::gap_md(ControlSize::Regular))
    row align:center gap:(shared::gap_sm(ControlSize::Regular))
        spinner a11y_label:"Exporting"
        text "Exporting Logo.png…"
    row align:center gap:(shared::gap_sm(ControlSize::Regular)) pad_x:(shared::gap_md(ControlSize::Regular)) pad_y:(shared::gap_sm(ControlSize::Regular)) fill:$theme.ink color:$theme.surface radius:(shared::radius_item())
        spinner a11y_label:"Saving"
        text "Saving version history"

[preview "A picture of the caller's"]
spinner tone:accent a11y_label:"Syncing"
    box width:32 height:32 radius:8 stroke:$text.ink stroke_width:3
