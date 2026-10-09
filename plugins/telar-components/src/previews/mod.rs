//! The catalogue's previews written in Rust, compiled only under `telar/previews`.

#[cfg(feature = "advanced")]
mod accordion;
mod badge;
mod button;
mod checkbox;
mod chip;
#[cfg(feature = "workbench")]
mod code_view;
#[cfg(feature = "workbench")]
mod color_picker;
#[cfg(feature = "overlays")]
mod command_palette;
#[cfg(feature = "overlays")]
mod context_menu;
#[cfg(feature = "overlays")]
mod drawer;
mod handle;
mod heading;
#[cfg(feature = "workbench")]
mod icon_button;
#[cfg(feature = "overlays")]
mod kbd;
mod line_gutter;
mod list;
#[cfg(feature = "overlays")]
mod menu;
#[cfg(feature = "overlays")]
mod modal;
mod progress;
mod radio;
#[cfg(feature = "advanced")]
mod reorder_zones;
#[cfg(feature = "advanced")]
mod reorderable;
mod sample;
mod scrub_field;
mod section;
mod select;
mod slider;
mod spinner;
#[cfg(feature = "workbench")]
mod split_pane;
#[cfg(feature = "advanced")]
mod stepper;
mod swatches;
mod tabs;
mod text_field;
#[cfg(feature = "overlays")]
mod toast;
mod toggle;
#[cfg(feature = "workbench")]
mod toolbar;
#[cfg(feature = "overlays")]
mod tooltip;
#[cfg(feature = "workbench")]
mod tree_view;
#[cfg(feature = "chrome")]
mod window_frame;

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;

use telar::preview::PreviewEntry;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    let mut entries = Vec::new();
    entries.extend(button::previews());
    entries.extend(text_field::previews());
    entries.extend(select::previews());
    entries.extend(checkbox::previews());
    entries.extend(toggle::previews());
    entries.extend(radio::previews());
    entries.extend(slider::previews());
    entries.extend(scrub_field::previews());
    entries.extend(swatches::previews());
    entries.extend(handle::previews());
    #[cfg(feature = "advanced")]
    entries.extend(stepper::previews());
    entries.extend(badge::previews());
    entries.extend(chip::previews());
    #[cfg(feature = "overlays")]
    entries.extend(kbd::previews());
    entries.extend(progress::previews());
    entries.extend(spinner::previews());
    entries.extend(heading::previews());
    entries.extend(section::previews());
    entries.extend(list::previews());
    #[cfg(feature = "advanced")]
    entries.extend(accordion::previews());
    #[cfg(feature = "advanced")]
    entries.extend(reorderable::previews());
    #[cfg(feature = "advanced")]
    entries.extend(reorder_zones::previews());
    entries.extend(tabs::previews());
    #[cfg(feature = "overlays")]
    entries.extend(modal::previews());
    #[cfg(feature = "overlays")]
    entries.extend(drawer::previews());
    #[cfg(feature = "overlays")]
    entries.extend(menu::previews());
    #[cfg(feature = "overlays")]
    entries.extend(context_menu::previews());
    #[cfg(feature = "overlays")]
    entries.extend(tooltip::previews());
    #[cfg(feature = "overlays")]
    entries.extend(command_palette::previews());
    #[cfg(feature = "overlays")]
    entries.extend(toast::previews());
    #[cfg(feature = "workbench")]
    entries.extend(icon_button::previews());
    #[cfg(feature = "workbench")]
    entries.extend(toolbar::previews());
    #[cfg(feature = "workbench")]
    entries.extend(split_pane::previews());
    #[cfg(feature = "workbench")]
    entries.extend(tree_view::previews());
    #[cfg(feature = "workbench")]
    entries.extend(color_picker::previews());
    #[cfg(feature = "workbench")]
    entries.extend(code_view::previews());
    entries.extend(line_gutter::previews());
    #[cfg(feature = "chrome")]
    entries.extend(window_frame::previews());
    entries
}
