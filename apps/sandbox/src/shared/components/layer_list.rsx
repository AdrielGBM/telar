[logic]
// The layers of a design, read through a plain function so no `$` in the view says they change.
#[derive(Default)]
pub struct Props {}

fn listed(layers: RwSignal<Vec<String>>) -> Vec<String> {
    layers.get()
}

let layers = signal(vec![String::from("Logo")]);

[view]
col gap:8
    box role:button a11y_label:"Add layer" on_press:(move || layers.update(|layers| layers.push(String::from("Frame 1")))) pad:8 radius:6 fill:$theme.surface
        text "Add layer"
    for layer in listed(layers)
        text "{layer}"
    match listed(layers).len()
        1
            text "One layer"
        count
            text "{count} layers"

[previews "Sandbox/Layer list" layout:padded]
A list and a count that follow the layers they read, with no `$` in the view to say they change.

[preview "Layer list"]
layer_list

[play]
canvas.expect(telar::preview::by_text("One layer"))?;
canvas.click(telar::preview::by_role(telar::Role::Button).named("Add layer"))?;
canvas.expect(telar::preview::by_text("Frame 1"))?;
canvas.expect(telar::preview::by_text("2 layers"))?;
canvas.expect_absent(telar::preview::by_text("One layer"))?;
