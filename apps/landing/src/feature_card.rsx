[logic]
pub struct Props {
    pub icon: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

[view]

box fill:surface stroke:border radius:16 width:300 min-width:260 grow:1 pad:24 gap:10 direction:col
    text "{props.icon}" size:32
    text "{props.title}" size:18 color:dark
    text "{props.body}" size:14 color:muted
