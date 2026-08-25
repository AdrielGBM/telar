[logic]
// One documentation row for an attribute: its name, accepted values, and a short description.
#[derive(Default)]
pub struct Props {
    pub name: &'static str,
    pub values: &'static str,
    pub about: &'static str,
}

[view]
row gap:12 align:start
    text "{props.name}" font_size:13 color:theme.ink width:120
    text "{props.values}" font_size:13 color:theme.primary width:190
    text "{props.about}" font_size:13 color:theme.muted grow:1

[preview "Prop row"]
prop_row name:"align" values:"start · center · end · stretch" about:"Cross-axis alignment of children."
