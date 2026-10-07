[logic]
#[derive(Default)]
pub struct Props {
    pub name: &'static str,
}

[view]
row gap:8 pad:12 align:center fill:$theme.primary radius:$theme.radius
    svg src:"mark.svg" width:24 height:24
    icon name:"fixture:dot" size:16
    text "{t!(\"greeting\", name = props.name)}" font_size:14 color:$theme.on_primary
