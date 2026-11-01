[logic]
use crate::metrics;
use telar::preview::{by_role, by_text};
use telar::{Color, ControlSize, Key, NamedKey, Role};

[previews "Display/Tag" layout:centered]

[preview "Default" matrix:themes]
tag label:"Tag" tone:accent variant:solid size:regular

[preview "Tones and variants" viewport:120x48 matrix:(mode variant tone)]
tag label:"Tag" tone:accent variant:solid

[preview "Statuses" values(variant:Variant::Solid) matrix:themes]
row gap:(metrics::gap_md(ControlSize::Regular))
    tag label:"Saved" tone:success variant:variant
    tag label:"Unsaved changes" tone:warning variant:variant
    tag label:"Export failed" tone:danger variant:variant
    tag label:"Beta" tone:info variant:variant

[preview "Custom colour" values(brand:Color::rgb(0.49, 0.23, 0.93)) matrix:themes]
row gap:(metrics::gap_md(ControlSize::Regular))
    tag label:"Brand" tone:brand variant:solid
    tag label:"Brand" tone:brand variant:soft
    tag label:"Brand" tone:brand variant:outline
    tag label:"Brand" tone:brand variant:ghost

[preview "Filters" args(images:true) tags:[stateful] matrix:themes]
row gap:(metrics::gap_md(ControlSize::Regular))
    tag label:"Images" selected:$images
    tag label:"Vectors" on_toggle:(move |_| actions.log("on_toggle", Vec::new()))

[play]
canvas.expect_toggled(by_role(Role::Button).named("Images"), true)?;
canvas.expect_toggled(by_role(Role::Button).named("Vectors"), false)?;
canvas.click(by_role(Role::Button).named("Vectors"))?;
canvas.expect_toggled(by_role(Role::Button).named("Vectors"), true)?;
canvas.tab_back()?;
canvas.expect_focused(by_role(Role::Button).named("Images"))?;
canvas.press(Key::Named(NamedKey::Space))?;
canvas.expect_toggled(by_role(Role::Button).named("Images"), false)?;
canvas.press(Key::Named(NamedKey::Enter))?;
canvas.expect_toggled(by_role(Role::Button).named("Images"), true)?;
canvas.expect_action("on_toggle", 1)?;

[preview "Counter"]
tag label:"3" variant:solid a11y_label:"3 unread comments"

[play]
canvas.expect(by_role(Role::Label).named("3 unread comments"))?;
canvas.expect_absent(by_text("3"))?;

[preview "Leading and trailing" matrix:themes]
row gap:(metrics::gap_md(ControlSize::Regular))
    tag label:"Saved" tone:success
        box slot:"leading" width:6 height:6 radius:3 fill:$text.ink
    tag label:"Exporting" tone:info variant:soft
        spinner slot:"leading" diameter:(metrics::icon_size(ControlSize::Regular) * 0.75) color:$theme.info
    tag label:"Layers" variant:outline
        text "12" slot:"trailing" font_weight:$theme.font_weight_strong

[preview "Removable layers"]
row gap:(metrics::gap_md(ControlSize::Regular))
    for label in ["Frame 1", "Logo"]
        tag label:label
            icon_button slot:"action" icon:(telar::icon!("lucide:x")) label:"Remove {label}" on_press:(move || actions.log("remove", vec![label.to_string()]))

[play]
canvas.click(by_role(Role::Button).named("Remove Logo"))?;
canvas.expect_action("remove", 1)?;
canvas.tab_back()?;
canvas.expect_focused(by_role(Role::Button).named("Remove Frame 1"))?;
canvas.press(Key::Named(NamedKey::Enter))?;
canvas.expect_action("remove", 2)?;
canvas.press(Key::Named(NamedKey::Space))?;
canvas.expect_action("remove", 3)?;

[preview "Small remove buttons" matrix:themes]
row gap:(metrics::gap_md(ControlSize::Regular))
    for label in ["Frame 1", "Logo"]
        tag label:label
            icon_button slot:"action" icon:(telar::icon!("lucide:x")) label:"Remove {label}" size:small on_press:(move || actions.log("remove", vec![label.to_string()]))

[play]
canvas.click(by_role(Role::Button).named("Remove Logo"))?;
canvas.expect_action("remove", 1)?;
canvas.tab_back()?;
canvas.expect_focused(by_role(Role::Button).named("Remove Frame 1"))?;
canvas.press(Key::Named(NamedKey::Enter))?;
canvas.expect_action("remove", 2)?;

[preview "Open in a new tab" matrix:themes]
tag label:"Logo.svg" tone:neutral variant:soft
    icon_button slot:"action" icon:(telar::icon!("lucide:external-link")) label:"Open Logo.svg in a new tab" on_press:(move || actions.log("open", Vec::new()))

[play]
canvas.click(by_role(Role::Button).named("Open Logo.svg in a new tab"))?;
canvas.expect_action("open", 1)?;
