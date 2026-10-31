//! The catalogue's previews written in Rust, compiled only under `telar/previews`.

mod accordion;
mod button;
mod checkbox;
mod code_view;
mod color_picker;
mod command_palette;
mod context_menu;
mod data_table;
mod drawer;
mod handle;
mod heading;
mod icon_button;
mod image;
mod kbd;
mod line_gutter;
mod link;
mod list;
mod menu;
mod modal;
mod progress;
mod radio;
mod reorder_zones;
mod reorderable;
mod resizable;
mod sample;
mod scrub_field;
mod search_field;
mod section;
mod select;
mod slider;
mod spinner;
mod split_pane;
mod stepper;
mod swatches;
mod tabs;
mod tag;
mod text_field;
mod toast;
mod toggle;
mod toolbar;
mod tooltip;
mod tree_view;
mod virtual_list;
mod window_controls;
mod window_frame;

#[cfg(test)]
#[path = "previews_test.rs"]
mod tests;

use telar::preview::PreviewEntry;

pub(crate) fn previews() -> Vec<PreviewEntry> {
    let mut entries = Vec::new();
    entries.extend(button::previews());
    entries.extend(text_field::previews());
    entries.extend(search_field::previews());
    entries.extend(select::previews());
    entries.extend(checkbox::previews());
    entries.extend(toggle::previews());
    entries.extend(radio::previews());
    entries.extend(slider::previews());
    entries.extend(scrub_field::previews());
    entries.extend(swatches::previews());
    entries.extend(handle::previews());
    entries.extend(stepper::previews());
    entries.extend(tag::previews());
    entries.extend(image::previews());
    entries.extend(link::previews());
    entries.extend(kbd::previews());
    entries.extend(progress::previews());
    entries.extend(spinner::previews());
    entries.extend(heading::previews());
    entries.extend(section::previews());
    entries.extend(list::previews());
    entries.extend(virtual_list::previews());
    entries.extend(data_table::previews());
    entries.extend(accordion::previews());
    entries.extend(reorderable::previews());
    entries.extend(reorder_zones::previews());
    entries.extend(tabs::previews());
    entries.extend(modal::previews());
    entries.extend(drawer::previews());
    entries.extend(menu::previews());
    entries.extend(context_menu::previews());
    entries.extend(tooltip::previews());
    entries.extend(command_palette::previews());
    entries.extend(toast::previews());
    entries.extend(icon_button::previews());
    entries.extend(toolbar::previews());
    entries.extend(split_pane::previews());
    entries.extend(resizable::previews());
    entries.extend(tree_view::previews());
    entries.extend(color_picker::previews());
    entries.extend(code_view::previews());
    entries.extend(line_gutter::previews());
    entries.extend(window_controls::previews());
    entries.extend(window_frame::previews());
    entries
}
