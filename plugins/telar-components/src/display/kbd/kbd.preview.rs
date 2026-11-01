use telar::preview::{Axis, Layout, Matrix, PreviewEntry, preview};
use telar::{
    BorderRadius, Container, LayoutError, LayoutItem, LayoutStyle, RectStyle, ShapeStyle,
    StyledContainer,
    Text, box_item, children, use_theme_tokens,
};

use crate::display::kbd::kbd::{KbdCaps, KbdProps, kbd};
use crate::shared;
use telar::{ControlSize, Tone, Variant};

/// A command beside its chord, on a surface of its own: the page's, or a tooltip's in the page's ink.
fn command(
    label: &'static str,
    chord: &'static str,
    tone: Tone,
    variant: Variant,
    inverted: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::declaring(
        move || label.to_string(),
        LayoutStyle::new(),
        |t| t,
    )?;
    let chord = kbd(KbdProps::props().chord(chord).tone(tone).variant(variant).build())?;
    let row = StyledContainer::new(
        LayoutStyle::row()
            .align_items(telar::AlignItems::CENTER)
            .justify_content(telar::JustifyContent::SPACE_BETWEEN)
            .gap(shared::gap_md(ControlSize::Regular))
            .padding_horizontal(shared::gap_md(ControlSize::Regular))
            .padding_vertical(shared::gap_sm(ControlSize::Regular)),
        move |_| {
            let theme = use_theme_tokens();
            let fill = match inverted {
                true => theme.ink(),
                false => theme.surface(),
            };
            RectStyle::default()
                .with_fill(fill)
                .with_radius(BorderRadius::all(shared::radius_item()))
        },
        children![text, chord],
    )?
    .declaring(move || {
        let theme = use_theme_tokens();
        let ink = match inverted {
            true => theme.surface(),
            false => theme.ink(),
        };
        telar::Declared::default().with_color(ink)
    });
    Ok(box_item(row))
}

pub(crate) fn previews() -> Vec<PreviewEntry> {
    vec![
        preview!(kbd: KbdProps, "Default", |p| {
            kbd(KbdProps::props()
                .chord(p.arg("chord", "Mod+Shift+P"))
                .size(p.arg("size", ControlSize::Regular))
                .caps(p.arg("caps", KbdCaps::Each))
                .tone(p.arg("tone", Tone::Ink))
                .variant(p.arg("variant", Variant::Soft))
                .build())
        })
        .title("Display/Kbd")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(kbd: KbdProps, "Tones and variants", |p| {
            kbd(KbdProps::props()
                .chord("Mod+D")
                .tone(p.arg("tone", Tone::Ink))
                .variant(p.arg("variant", Variant::Soft))
                .build())
        })
        .title("Display/Kbd")
        .layout(Layout::Centered)
        .viewport(120.0, 48.0)
        .matrix(Matrix::Axes(&[
            Axis::RegisteredModes,
            Axis::ArgValues("variant"),
            Axis::ArgValues("tone"),
        ])),
        preview!(kbd: KbdProps, "A sequence", |p| {
            kbd(KbdProps::props()
                .chord(p.arg("chord", "Mod+K H"))
                .caps(p.arg("caps", KbdCaps::Each))
                .build())
        })
        .title("Display/Kbd")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
        preview!(kbd: KbdProps, "In context", |p| {
            let variant = p.arg("variant", Variant::Soft);
            let tone = p.arg("tone", Tone::Ink);
            let column = Container::new(
                LayoutStyle::column().gap(shared::gap_md(ControlSize::Regular)),
                vec![
                    command("Duplicate", "Mod+D", tone.clone(), variant, false)?,
                    command("Group selection", "Mod+G", tone, variant, true)?,
                ],
            )?;
            Ok(box_item(column))
        })
        .title("Display/Kbd")
        .layout(Layout::Centered)
        .matrix(Matrix::Named("themes")),
    ]
}
