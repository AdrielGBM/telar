use std::rc::Rc;

use telar::preview::{Axis, Layout, Matrix, PreviewEntry, by_role, by_text, preview};
use telar::{Accessible, Color, Container, Key, LayoutError, LayoutStyle, NamedKey, Role};

use crate::metrics;
use crate::display::tag::tag::{TagProps, tag};
use telar::{ControlSize, Tone, Variant};

fn row(tags: Vec<Box<dyn telar::LayoutItem>>) -> Result<Container, LayoutError> {
    let style = || LayoutStyle::row().gap(metrics::gap_md(ControlSize::Regular));
    Ok(Container::new(style(), tags)?.styled_by(style))
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    let entries = vec![
        preview!(tag: TagProps, "Default", |p| {
            tag(
                TagProps::props()
                    .label(p.arg("label", "Tag"))
                    .tone(p.arg("tone", Tone::Accent))
                    .variant(p.arg("variant", Variant::Solid))
                    .size(p.arg("size", ControlSize::Regular))
                    .build(),
            )
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(tag: TagProps, "Tones and variants", |p| {
            tag(
                TagProps::props()
                    .label("Tag")
                    .tone(p.arg("tone", Tone::Accent))
                    .variant(p.arg("variant", Variant::Solid))
                    .build(),
            )
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .viewport(120.0, 48.0)
        .matrix(Matrix::Axes(&[
            Axis::RegisteredModes,
            Axis::ArgValues("variant"),
            Axis::ArgValues("tone"),
        ])),
        preview!(tag: TagProps, "Statuses", |p| {
            let variant = p.arg("variant", Variant::Solid);
            let tags = [
                ("Saved", Tone::Success),
                ("Unsaved changes", Tone::Warning),
                ("Export failed", Tone::Danger),
                ("Beta", Tone::Info),
            ]
            .into_iter()
            .map(|(label, tone)| {
                tag(
                    TagProps::props()
                        .label(label)
                        .tone(tone)
                        .variant(variant)
                        .build(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
            row(tags)
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(tag: TagProps, "Custom colour", |p| {
            let brand = p.arg("brand", Color::rgb(0.49, 0.23, 0.93));
            let tags = [Variant::Solid, Variant::Soft, Variant::Outline, Variant::Ghost]
                .into_iter()
                .map(|variant| {
                    tag(
                        TagProps::props()
                            .label("Brand")
                            .tone(brand)
                            .variant(variant)
                            .build(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            row(tags)
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(tag: TagProps, "Filters", |p| {
            let bound = tag(
                TagProps::props()
                    .label("Images")
                    .selected(p.signal("images", true))
                    .build(),
            )?;
            let uncontrolled = tag(
                TagProps::props()
                    .label("Vectors")
                    .on_toggle(Rc::new(|_| {}))
                    .build(),
            )?;
            row(vec![bound, uncontrolled])
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .tags(&["stateful"])
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
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
            canvas.expect_action("on_toggle", 1)
        }),
        preview!(tag: TagProps, "Counter", |_p| {
            Ok::<_, LayoutError>(
                tag(TagProps::props().label("3").variant(Variant::Solid).build())?
                    .a11y_label(|| "3 unread comments"),
            )
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .play(|canvas| {
            canvas.expect(by_role(Role::Label).named("3 unread comments"))?;
            canvas.expect_absent(by_text("3"))
        }),
    ];
    #[cfg(feature = "icons")]
    let entries = [entries, vec![with_slots()]].concat();
    [entries, with_actions()].concat()
}

fn with_actions() -> Vec<PreviewEntry> {
    vec![
        preview!(tag: TagProps, "Removable layers", |p| {
            let actions = p.actions();
            let tags = ["Frame 1", "Logo"]
                .into_iter()
                .map(|label| {
                    let remove = action(
                        telar::icon!("lucide:x"),
                        format!("Remove {label}"),
                        ControlSize::Regular,
                        Rc::new(move || actions.log("remove", vec![label.to_string()])),
                    );
                    tag(TagProps::props().label(label).action(remove).build())
                })
                .collect::<Result<Vec<_>, _>>()?;
            row(tags)
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .play(|canvas| {
            canvas.click(by_role(Role::Button).named("Remove Logo"))?;
            canvas.expect_action("remove", 1)?;
            canvas.tab_back()?;
            canvas.expect_focused(by_role(Role::Button).named("Remove Frame 1"))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_action("remove", 2)?;
            canvas.press(Key::Named(NamedKey::Space))?;
            canvas.expect_action("remove", 3)
        }),
        preview!(tag: TagProps, "Small remove buttons", |p| {
            let actions = p.actions();
            let tags = ["Frame 1", "Logo"]
                .into_iter()
                .map(|label| {
                    let remove = action(
                        telar::icon!("lucide:x"),
                        format!("Remove {label}"),
                        ControlSize::Small,
                        Rc::new(move || actions.log("remove", vec![label.to_string()])),
                    );
                    tag(TagProps::props().label(label).action(remove).build())
                })
                .collect::<Result<Vec<_>, _>>()?;
            row(tags)
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.click(by_role(Role::Button).named("Remove Logo"))?;
            canvas.expect_action("remove", 1)?;
            canvas.tab_back()?;
            canvas.expect_focused(by_role(Role::Button).named("Remove Frame 1"))?;
            canvas.press(Key::Named(NamedKey::Enter))?;
            canvas.expect_action("remove", 2)
        }),
        preview!(tag: TagProps, "Open in a new tab", |p| {
            let actions = p.actions();
            let open = action(
                telar::icon!("lucide:external-link"),
                "Open Logo.svg in a new tab".to_string(),
                ControlSize::Regular,
                Rc::new(move || actions.log("open", Vec::new())),
            );
            tag(
                TagProps::props()
                    .label("Logo.svg")
                    .tone(Tone::Neutral)
                    .variant(Variant::Soft)
                    .action(open)
                    .build(),
            )
        })
        .title("Display/Tag")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes"))
        .play(|canvas| {
            canvas.click(by_role(Role::Button).named("Open Logo.svg in a new tab"))?;
            canvas.expect_action("open", 1)
        }),
    ]
}

fn action(
    icon: telar::IconName,
    label: String,
    size: ControlSize,
    run: Rc<dyn Fn()>,
) -> telar::Children {
    telar::Children::one(move || {
        crate::icon_button(
            crate::IconButtonProps::props()
                .icon(icon.clone())
                .label(label.clone())
                .size(size)
                .on_press(run.clone())
                .build(),
        )
    })
}

#[cfg(feature = "icons")]
fn with_slots() -> PreviewEntry {
    preview!(tag: TagProps, "Leading and trailing", |_p| {
        let saved = tag(
            TagProps::props()
                .label("Saved")
                .tone(Tone::Success)
                .leading(drawn(|| icon(telar::icon!("lucide:check"))))
                .build(),
        )?;
        let exporting = tag(
            TagProps::props()
                .label("Exporting")
                .tone(Tone::Info)
                .variant(Variant::Soft)
                .leading(drawn(|| {
                    crate::spinner(
                        crate::SpinnerProps::props()
                            .diameter(glyph_size())
                            .color(telar::Reactive::of(|| telar::use_theme_tokens().info()))
                            .build(),
                    )
                }))
                .build(),
        )?;
        let logo = tag(
            TagProps::props()
                .label("Logo")
                .variant(Variant::Outline)
                .trailing(drawn(|| icon(telar::icon!("lucide:star"))))
                .build(),
        )?;
        row(vec![saved, exporting, logo])
    })
    .title("Display/Tag")
    .layout(Layout::Centered)
    .matrix(Matrix::Named("themes"))
}

#[cfg(feature = "icons")]
fn drawn(
    build: impl Fn() -> Result<Box<dyn telar::LayoutItem>, LayoutError> + 'static,
) -> telar::Children {
    telar::Children::one(build)
}

#[cfg(feature = "icons")]
fn glyph_size() -> f32 {
    metrics::icon_size(ControlSize::Regular) * 0.75
}

#[cfg(feature = "icons")]
fn icon(name: telar::IconName) -> Result<Box<dyn telar::LayoutItem>, LayoutError> {
    telar_icons::icon(
        telar_icons::IconProps::props()
            .name(name)
            .size(glyph_size())
            .build(),
    )
}
