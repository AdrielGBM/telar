[logic]
let shown = signal(true);
let reading = move |text: &'static str| shown.get().then(|| text.to_string());

[view]
col gap:16 pad:24 align:start fill:$theme.surface
    text font_size:28 color:$theme.ink
        span "The "
        span "colonel" ruby:(reading("KER-nl"))
        span " led the "
        span "choir" ruby:(reading("KWIRE"))
        span " home."
    text font_size:18 color:$theme.muted
        span "A reading far wider than its base: "
        span "I" ruby:(reading("first person"))
        span " shrink it, close it up, then widen."
    button label:"Toggle readings" on_press:(move || shown.set(!shown.get()))

[previews "Text/Ruby" layout:padded]
A reading set above a run of a paragraph: `span "colonel" ruby:"KER-nl"`. The line grows to hold it, the run never breaks across lines, a reading wider than its run shrinks and closes up before it widens it, and a reader hears the run and then its reading.

[preview "Pronunciation" matrix:themes]
reading_guide

[play]
canvas.expect(by_text("The colonel(KER-nl) led the choir(KWIRE) home."))?;
canvas.click(by_role(Role::Button).named("Toggle readings"))?;
canvas.expect(by_text("The colonel led the choir home."))?;
canvas.click(by_role(Role::Button).named("Toggle readings"))?;
canvas.expect(by_text("A reading far wider than its base: I(first person) shrink it, close it up, then widen."))?;
