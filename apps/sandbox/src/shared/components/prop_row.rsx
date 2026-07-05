[logic]
// One documentation row for an attribute: its name, accepted values, and a short description.
pub struct Props {
    pub name: &'static str,
    pub values: &'static str,
    pub about: &'static str,
}

[view]
row gap:12 align:start
    text "{props.name}" size:13 color:ink width:120
    text "{props.values}" size:13 color:primary width:190
    text "{props.about}" size:13 color:muted grow:1

[preview "Prop row"]
prop_row name:"align" values:"start · center · end · stretch" about:"Cross-axis alignment of children."
