//! The catalogue's previews written in Rust, compiled only under `telar/previews`.

#[path = "../layout/accordion/accordion.previews.rs"]
mod accordion;
#[path = "../inputs/button/button.previews.rs"]
mod button;
#[path = "../inputs/checkbox/checkbox.previews.rs"]
mod checkbox;
#[path = "../workbench/code_view/code_view.previews.rs"]
mod code_view;
#[path = "../workbench/color_picker/color_picker.previews.rs"]
mod color_picker;
#[path = "../overlays/command_palette/command_palette.previews.rs"]
mod command_palette;
#[path = "../overlays/menu/context_menu.previews.rs"]
mod context_menu;
#[path = "../display/data_table/data_table.previews.rs"]
mod data_table;
#[path = "../overlays/drawer/drawer.previews.rs"]
mod drawer;
#[path = "../inputs/handle/handle.previews.rs"]
mod handle;
#[path = "../layout/heading/heading.previews.rs"]
mod heading;
#[path = "../workbench/icon_button/icon_button.previews.rs"]
mod icon_button;
#[path = "../display/image/image.previews.rs"]
mod image;
#[path = "../display/kbd/kbd.previews.rs"]
mod kbd;
#[path = "../workbench/line_gutter/line_gutter.previews.rs"]
mod line_gutter;
#[path = "../navigation/link/link.previews.rs"]
mod link;
#[path = "../layout/list/list.previews.rs"]
mod list;
#[path = "../overlays/menu/menu.previews.rs"]
mod menu;
#[path = "../overlays/modal/modal.previews.rs"]
mod modal;
#[path = "../feedback/progress/progress.previews.rs"]
mod progress;
#[path = "../inputs/radio/radio.previews.rs"]
mod radio;
#[path = "../layout/reorder_zones/reorder_zones.previews.rs"]
mod reorder_zones;
#[path = "../layout/reorderable/reorderable.previews.rs"]
mod reorderable;
#[path = "../workbench/resizable/resizable.previews.rs"]
mod resizable;
mod sample;
#[path = "../inputs/scrub_field/scrub_field.previews.rs"]
mod scrub_field;
#[path = "../inputs/search_field/search_field.previews.rs"]
mod search_field;
#[path = "../layout/section/section.previews.rs"]
mod section;
#[path = "../inputs/select/select.previews.rs"]
mod select;
#[path = "../inputs/slider/slider.previews.rs"]
mod slider;
#[path = "../feedback/spinner/spinner.previews.rs"]
mod spinner;
#[path = "../workbench/split_pane/split_pane.previews.rs"]
mod split_pane;
#[path = "../inputs/stepper/stepper.previews.rs"]
mod stepper;
#[path = "../inputs/swatches/swatches.previews.rs"]
mod swatches;
#[path = "../navigation/tabs/tabs.previews.rs"]
mod tabs;
#[path = "../display/tag/tag.previews.rs"]
mod tag;
#[path = "../inputs/text_field/text_field.previews.rs"]
mod text_field;
#[path = "../overlays/toast/toast.previews.rs"]
mod toast;
#[path = "../inputs/toggle/toggle.previews.rs"]
mod toggle;
#[path = "../workbench/toolbar/toolbar.previews.rs"]
mod toolbar;
#[path = "../overlays/tooltip/tooltip.previews.rs"]
mod tooltip;
#[path = "../workbench/tree_view/tree_view.previews.rs"]
mod tree_view;
#[path = "../layout/virtual_list/virtual_list.previews.rs"]
mod virtual_list;
#[path = "../chrome/title_bar/window_controls.previews.rs"]
mod window_controls;
#[path = "../chrome/window_frame/window_frame.previews.rs"]
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
