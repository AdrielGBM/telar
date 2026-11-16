[logic]
use crate::shared;
use telar::preview::by_role;
use telar::{ControlSize, Role};

[previews "Feedback/Progress" layout:centered]
Use a spinner when the work cannot say how far along it is, a progress bar when it can.

[preview "Default" matrix:themes]
progress value:0.6 size:regular tone:accent a11y_label:"Exporting"

[play]
canvas.expect_value(by_role(Role::ProgressBar).named("Exporting"), 60.0)?;

[preview "Tones and sizes" viewport:280x48 matrix:(mode size tone)]
progress value:0.6 size:regular tone:accent a11y_label:"Exporting"

[preview "In context" matrix:themes]
col gap:(shared::gap_sm(ControlSize::Regular)) width:320 pad:(shared::gap_lg(ControlSize::Regular)) fill:$theme.surface radius:(shared::radius_surface())
    row justify:between
        text "Exporting Logo.png"
        text "60%" color:$theme.muted
    progress value:0.6 stretch a11y_label:"Exporting Logo.png"

[preview "Right to left" dir:rtl]
progress value:0.3 a11y_label:"Exporting"
