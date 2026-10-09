use std::rc::Rc;
use std::time::Duration;

use telar::preview::{Layout, Matrix, PreviewEntry, preview};
use telar::{Children, Container, LayoutError, LayoutStyle, box_item};

use crate::button::{ButtonProps, button};
use crate::toast::{Toast, ToastKind, ToasterProps, Toasts, toaster};

fn poster(
    label: &'static str,
    queue: Toasts,
    toast: impl Fn() -> Toast + 'static,
) -> Result<Box<dyn telar::LayoutItem>, LayoutError> {
    button(
        ButtonProps::props()
            .label(label)
            .on_press(Rc::new(move || {
                queue.push(toast());
            }))
            .build(),
        Children::default(),
    )
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(toaster: ToasterProps, "Stack", |p| {
            let queue = Toasts::new();
            let seconds = p.arg("seconds", 5u64);
            let lasting = Duration::from_secs(seconds.max(1));
            queue.push(
                Toast::new("Your changes are saved.")
                    .titled("Saved")
                    .kind(ToastKind::Success)
                    .until_dismissed(),
            );
            let buttons = vec![
                poster("Info", queue, move || {
                    Toast::new("A new version is available.").lasting(lasting)
                })?,
                poster("Error", queue, move || {
                    Toast::new("The file could not be written.")
                        .titled("Save failed")
                        .kind(ToastKind::Error)
                        .lasting(lasting)
                        .with_action("Retry", || {})
                })?,
            ];
            let row = Container::new(LayoutStyle::new().flex_row().gap(8.0), buttons)?;
            let stack = toaster(
                ToasterProps::props()
                    .toasts(queue)
                    .max_visible(p.arg("max_visible", 3u32))
                    .build(),
                Children::default(),
            )?;
            Ok::<_, LayoutError>(Container::new(
                LayoutStyle::new().flex_column(),
                vec![box_item(row), stack],
            )?)
        })
        .title("Overlays/Toast")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .tags(&["stateful"]),
    ]
}
