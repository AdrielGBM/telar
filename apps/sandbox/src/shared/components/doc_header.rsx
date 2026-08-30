[logic]
#[derive(Default)]
pub struct Props {
    pub kicker: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
}

[view]
col gap:6
    text "{props.kicker}" font_size:12 color:$theme.primary
    text "{props.title}" font_size:26 color:$theme.ink
    text "{props.desc}" font_size:14 color:$theme.muted max_width:760

[preview "Doc header"]
doc_header kicker:"FOUNDATIONS" title:"Layout" desc:"Flexbox rows and columns with gaps, padding, alignment and growth."
