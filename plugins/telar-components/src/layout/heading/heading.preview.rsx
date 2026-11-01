[logic]
use crate::metrics;
use telar::preview::by_role;
use telar::{ControlSize, Role};

[previews "Layout/Heading" layout:centered]

[preview "Default" matrix:themes]
heading text:"Heading" level:2

[preview "Levels"]
col gap:(metrics::gap_md(ControlSize::Regular))
    for level in 1..=6u8
        heading text:"Heading {level}" level:level

[play]
for level in 1..=6u8 {
    canvas.expect(by_role(Role::Heading(level)).named(format!("Heading {level}")))?;
}
