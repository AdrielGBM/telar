use std::rc::Rc;

use telar::preview::{Layout, Matrix, PreviewEntry, by_role, preview};
use super::sample::Given;
use telar::{Color, Container, LayoutError, LayoutStyle, Role};

use crate::metrics;
use crate::inputs::radio::radio::{RadioProps, radio};
use telar::ControlSize;

const SIZES: [&str; 3] = ["Small", "Medium", "Large"];

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(radio: RadioProps, "Group", |p| {
            let selected = p.signal("selected", Some(1usize));
            let color = p.arg("color", None::<Color>);
            let size = p.arg("size", ControlSize::Regular);
            let options = (0usize..)
                .zip(SIZES)
                .map(|(value, label)| {
                    radio(
                        RadioProps::props()
                            .on_select(Rc::new(|_| {}))
                            .selected(selected)
                            .value(value)
                            .label(label)
                            .size(size)
                            .given(color, |props, color| props.color(color))
                            .build(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::column().gap(metrics::gap_xs(size)),
                options,
            )?)
        })
        .title("Inputs/Radio")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"])
        .play(|canvas| {
            canvas.click(by_role(Role::Radio).named("Large"))?;
            canvas.expect_toggled(by_role(Role::Radio).named("Large"), true)?;
            canvas.expect_toggled(by_role(Role::Radio).named("Medium"), false)?;
            canvas.expect_action("on_select", 1)
        }),
    ]
}
