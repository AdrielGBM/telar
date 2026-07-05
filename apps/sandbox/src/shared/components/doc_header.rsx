[logic]
#[derive(Default)]
pub struct Props {
    pub kicker: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
}

[view]
col gap:6
    text "{props.kicker}" size:12 color:primary
    text "{props.title}" size:26 color:ink
    text "{props.desc}" size:14 color:muted max_width:760

[preview "Doc header"]
doc_header kicker:"01 · FOUNDATIONS" title:"Layout" desc:"Flexbox rows and columns with gaps, padding, alignment and growth."
