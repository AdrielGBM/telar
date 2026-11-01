[previews "Fixture/Tally" layout:centered tags:[rsx]]
The tally written in Rust, previewed from `.rsx`.

[preview "Rsx literal" matrix:(dir:[ltr rtl] count:[1 2])]
tally label:"Pears" count:2u32

[preview "Rsx bound" args(count:3u32) layout:padded]
tally label:"Plums" count:$count

[play]
canvas.expect_text("Plums · 3")?;
