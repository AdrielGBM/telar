[logic]
use crate::shared;
use telar::preview::by_role;
use telar::{ControlSize, Link, Location, Role, external};

[previews "Navigation/Link" layout:centered]
A link goes somewhere: a page of the application, a place on this one, or an address outside it. Use a `button` for what acts here instead.

[preview "Default" matrix:themes]
link label:"Link" to:(Location::root().segment("guide")) current:false tone:accent

[play]
canvas.expect(by_role(Role::Link(Link::default())).named("Link"))?;
canvas.tab()?;
canvas.expect_focused(by_role(Role::Link(Link::default())).named("Link"))?;

[preview "Tones" viewport:160x48 matrix:(mode tone)]
link label:"Version history" to:(Location::root().segment("history")) tone:accent

[preview "Leaving the application" matrix:themes]
link label:"Copy as CSS reference" to:(external("https://example.com/css"))

[play]
canvas.expect(by_role(Role::Link(Link::default())).named("Copy as CSS reference (external link)"))?;

[preview "In a navigation" matrix:themes]
row gap:(shared::gap_lg(ControlSize::Regular)) role:navigation a11y_label:"Editor"
    link label:"Design" to:(Location::root().segment("design")) current
    link label:"Prototype" to:(Location::root().segment("prototype"))
    link label:"Assets" to:(Location::root().segment("assets"))
        text "12" slot:"trailing" color:$theme.muted

[preview "In the text around it" matrix:themes]
col gap:(shared::gap_md(ControlSize::Regular))
    row align:center gap:(shared::gap_xs(ControlSize::Regular)) font_size:12 color:$theme.muted
        text "File saved."
        link label:"Open version history" to:(Location::root().segment("history")) tone:ink
    row align:center gap:(shared::gap_sm(ControlSize::Regular)) font_size:20
        text "Export failed."
        link label:"Try again" to:(Location::root().segment("export")) tone:danger
