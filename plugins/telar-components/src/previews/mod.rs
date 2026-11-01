//! The catalogue's previews written in Rust, compiled only under `telar/previews`.

#[path = "../layout/accordion/accordion.preview.rs"]
mod accordion;
#[path = "../inputs/button/button.preview.rs"]
mod button;
#[path = "../inputs/checkbox/checkbox.preview.rs"]
mod checkbox;
#[path = "../workbench/code_view/code_view.preview.rs"]
mod code_view;
#[path = "../workbench/color_picker/color_picker.preview.rs"]
mod color_picker;
#[path = "../overlays/command_palette/command_palette.preview.rs"]
mod command_palette;
#[path = "../overlays/menu/context_menu.preview.rs"]
mod context_menu;
#[path = "../display/data_table/data_table.preview.rs"]
mod data_table;
#[path = "../overlays/drawer/drawer.preview.rs"]
mod drawer;
#[path = "../inputs/handle/handle.preview.rs"]
mod handle;
#[path = "../layout/heading/heading.preview.rs"]
mod heading;
#[path = "../workbench/icon_button/icon_button.preview.rs"]
mod icon_button;
#[path = "../display/image/image.preview.rs"]
mod image;
#[path = "../display/kbd/kbd.preview.rs"]
mod kbd;
#[path = "../workbench/line_gutter/line_gutter.preview.rs"]
mod line_gutter;
#[path = "../navigation/link/link.preview.rs"]
mod link;
#[path = "../layout/list/list.preview.rs"]
mod list;
#[path = "../overlays/menu/menu.preview.rs"]
mod menu;
#[path = "../overlays/modal/modal.preview.rs"]
mod modal;
#[path = "../feedback/progress/progress.preview.rs"]
mod progress;
#[path = "../inputs/radio/radio.preview.rs"]
mod radio;
#[path = "../layout/reorder_zones/reorder_zones.preview.rs"]
mod reorder_zones;
#[path = "../layout/reorderable/reorderable.preview.rs"]
mod reorderable;
#[path = "../workbench/resizable/resizable.preview.rs"]
mod resizable;
mod sample;
#[path = "../inputs/scrub_field/scrub_field.preview.rs"]
mod scrub_field;
#[path = "../inputs/search_field/search_field.preview.rs"]
mod search_field;
#[path = "../layout/section/section.preview.rs"]
mod section;
#[path = "../inputs/select/select.preview.rs"]
mod select;
#[path = "../inputs/slider/slider.preview.rs"]
mod slider;
#[path = "../feedback/spinner/spinner.preview.rs"]
mod spinner;
#[path = "../workbench/split_pane/split_pane.preview.rs"]
mod split_pane;
#[path = "../inputs/stepper/stepper.preview.rs"]
mod stepper;
#[path = "../inputs/swatches/swatches.preview.rs"]
mod swatches;
#[path = "../navigation/tabs/tabs.preview.rs"]
mod tabs;
#[path = "../display/tag/tag.preview.rs"]
mod tag;
#[path = "../inputs/text_field/text_field.preview.rs"]
mod text_field;
#[path = "../overlays/toast/toast.preview.rs"]
mod toast;
#[path = "../inputs/toggle/toggle.preview.rs"]
mod toggle;
#[path = "../workbench/toolbar/toolbar.preview.rs"]
mod toolbar;
#[path = "../overlays/tooltip/tooltip.preview.rs"]
mod tooltip;
#[path = "../workbench/tree_view/tree_view.preview.rs"]
mod tree_view;
#[path = "../layout/virtual_list/virtual_list.preview.rs"]
mod virtual_list;
#[path = "../chrome/title_bar/window_controls.preview.rs"]
mod window_controls;
#[path = "../chrome/window_frame/window_frame.preview.rs"]
mod window_frame;

#[cfg(test)]
#[path = "previews.test.rs"]
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
