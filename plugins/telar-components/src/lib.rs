//! The first-party widget catalogue for Telar: buttons, fields, selects, tabs, sliders, menus, modals, accordions and the rest, built on the kernel's primitives through the `telar` facade the way an application's own components are.
//!
//! An application depends on it beside `telar`. Every widget is in every build, and an application links only the ones it calls; a feature brings in a dependency, and none is on by default:
//!
//! - `icons`: draws `icon:"set:name"` ids through `telar-icons`, baked with the application. Without it, icon buttons and icon tabs take the same ids, a [`telar::IconName`] in every build, and draw a Unicode stand-in read from the icon's name.
//! - `async-assets`: lets an [`image`] follow an `AssetLoader`, whose load state converts into what the image shows.
//!
//! ```toml
//! # Cargo.toml
//! telar-components = { version = "0.2.3", features = ["icons"] }
//!
//! # telar.toml
//! [telar]
//! prelude = ["telar_components"]
//! ```
//!
//! The prelude entry is what lets `.rsx` call `button` or `modal` without a `use`: generated code glob-imports it after `telar`'s own items.
//!
//! **Keep this crate on the same version as `telar`.** Every kernel type a widget here takes or returns is reached through the facade, so a mismatch resolves two copies of the kernel and a widget built here stops being the `LayoutItem` a tree over there accepts — a type error naming one trait twice. It is the same lockstep `telar` and `telar-macros` already have, and for the same reason.

#![warn(rustdoc::broken_intra_doc_links)]

telar::rsx_modules!();

#[cfg(test)]
mod consistency_test;
#[cfg(test)]
#[path = "layout/heading/heading_test.rs"]
mod heading_test;
#[cfg(test)]
#[path = "display/kbd/kbd_test.rs"]
mod kbd_test;
mod look;
mod shared;
mod strings;
#[cfg(test)]
mod style_scan_test;
#[cfg(test)]
#[path = "display/tag/tag_test.rs"]
mod tag_test;
#[cfg(test)]
mod test_support;

pub use chrome::title_bar::title_bar::{
    TitleBarOptions, WindowControlsProps, title_bar, window_controls,
};
pub use chrome::window_frame::window_edges::window_edges;
pub use chrome::window_frame::window_frame::{
    MIN_FRAME_SIZE, SurfaceFrameStyle, WindowControls, WindowFrameProps, window_frame,
};
pub use display::data_table::data_table::{
    CellAlign, CellBuilder, Column, ColumnWidth, ColumnWidths, Comparator, DataTableProps,
    DataTableTokens, SortHandler, TableRow, TableRowHandler, TableSort, data_table,
};
pub use display::image::image::{ImageProps, ImageSource, ImageTokens, image};
pub use display::kbd::kbd::{KbdCaps, KbdProps, KbdTokens, kbd};
pub use display::tag::tag::{TagProps, TagTokens, tag};
pub use feedback::progress::progress::{ProgressProps, ProgressTokens, progress};
pub use feedback::spinner::spinner::{SpinnerProps, SpinnerTokens, spinner};
pub use inputs::button::button::{ButtonProps, ButtonTokens, button};
pub use inputs::checkbox::checkbox::{CheckboxProps, CheckboxTokens, checkbox};
pub use inputs::handle::handle::{HandleProps, HandleTokens, ToPoint, ToValue, handle};
pub use inputs::radio::radio::{RadioProps, RadioTokens, radio};
pub use inputs::scrub_field::scrub_field::{
    Format, Parse, ScrubFieldProps, ScrubFieldTokens, scrub_field,
};
pub use inputs::search_field::search_field::{SearchFieldProps, SearchHandler, search_field};
pub use inputs::select::select::{SelectProps, SelectTokens, select};
pub use inputs::slider::slider::{SliderProps, SliderTokens, slider};
pub use inputs::stepper::stepper::{StepperProps, StepperTokens, stepper};
pub use inputs::swatches::swatches::{SwatchesProps, SwatchesTokens, swatches};
pub use inputs::text_field::text_field::{
    FieldKeyHandler, TextFieldProps, TextFieldTokens, text_field,
};
pub use inputs::toggle::toggle::{ToggleProps, ToggleTokens, toggle};
pub use layout::accordion::accordion::{AccordionProps, AccordionTokens, accordion};
pub use layout::heading::heading::{HeadingProps, heading};
pub use layout::list::list::{
    GroupProps, ItemProps, ListTokens, SeparatorProps, group, item, separator,
};
pub use layout::reorder_zones::reorder_zones::{
    ReorderGroup, ReorderZoneProps, Slot, apply_zone_move,
};
pub use layout::reorderable::reorderable::{ItemBuilder, ReorderableProps, reorderable};
pub use layout::section::section::{SectionProps, section};
pub use layout::virtual_list::virtual_list::{
    ListRow, RowBuilder, RowHandler, VirtualListProps, virtual_list,
};
pub use navigation::link::link::{LinkProps, LinkTarget, LinkTokens, link};
pub use navigation::tabs::tabs::{Tab, TabsProps, TabsTokens, tab_list, tabs};
pub use overlays::command_palette::command_palette::{
    Command, CommandPaletteProps, CommandPaletteTokens, RunHandler, command_palette,
};
pub use overlays::command_palette::fuzzy::{FuzzyMatch, fuzzy_match};
pub use overlays::drawer::drawer::{DrawerProps, DrawerTokens, drawer};
pub use overlays::menu::menu::{
    ContextMenuProps, MenuProps, MenuTokens, MenuTriggerTokens, context_menu, focused_rect,
    is_context_menu_key, menu,
};
pub use overlays::menu::menu_panel::{SubmenuProps, submenu};
pub use overlays::modal::modal::{ModalProps, ModalTokens, modal};
pub use overlays::toast::toast::{
    Toast, ToastAction, ToastId, ToastPlacement, ToastTokens, ToasterProps, Toasts, show_toast,
    toaster, toasts,
};
pub use overlays::tooltip::tooltip::{TooltipProps, TooltipTokens, tooltip};
pub use shared::DOUBLE_CLICK;
pub use shared::selection::{SelectionHandler, SelectionMode};
pub use workbench::code_view::code_view::{
    CodeSpan, CodeViewProps, CodeViewTokens, CopyHandler, TokenKind, TokenStyler, code_view,
};
pub use workbench::color_picker::color_picker::{
    ColorPickerProps, ColorPickerTokens, color_picker,
};
pub use workbench::icon_button::icon_button::{IconButtonProps, IconButtonTokens, icon_button};
pub use workbench::line_gutter::line_gutter::LineGutter;
pub use workbench::resizable::resizable::{ResizableProps, ResizeEdges, resizable};
pub use workbench::resizable::resizable::{
    ResizableTokens, ResizeEdge, ResizeGrowth, ResizeHandleProps, resize_handle,
};
pub use workbench::split_pane::split_pane::{
    SizedPane, SplitDirection, SplitPaneProps, SplitPaneTokens, SplitRegion, split_pane,
};
pub use workbench::toolbar::toolbar::{ToolbarProps, toolbar};
pub use workbench::tree_view::tree_view::{
    ContextHandler, SelectHandler, TrailingBuilder, TreeContext, TreeNode, TreeViewProps,
    TreeViewTokens, tree_view,
};

/// The geometry, washes, type and marks every catalogue control is drawn from, for an application drawing controls of its own that have to sit beside the catalogue's.
///
/// Each one reads the theme in force — a role such as `radius_control` or `highlight_low`, never a literal — and every length is scaled by the [`ControlSize`](telar::ControlSize) it is handed, the size the control being drawn is written at, so a control built from these matches the catalogue's of that size at any theme:
///
/// ```ignore
/// use telar::ControlSize;
/// use telar_components::metrics;
///
/// let shell = LayoutStyle::new()
///     .min_height(metrics::control_height(ControlSize::Regular))
///     .padding_horizontal(metrics::control_pad_x(ControlSize::Regular));
/// let paint = |_r| RectStyle::default()
///     .with_border(Border::uniform(border(), metrics::border_width()))
///     .with_radius(BorderRadius::all(metrics::radius_control()));
/// ```
///
/// Read them inside the closure that builds the style, so a theme switch re-runs it. Every control height is at least [`MIN_TARGET`](metrics::MIN_TARGET); a mark or an icon a pointer operates on its own, drawn smaller, grows its target by [`target_outset`](metrics::target_outset).
pub mod metrics {
    pub use crate::shared::{
        MIN_TARGET, Mark, border_width, border_width_strong, control_height, control_pad_x,
        control_text, focus_ring, gap_lg, gap_md, gap_sm, gap_xl, gap_xs, held_wash, hover_wash,
        hovered_fill, icon_pad, icon_size, ink_on, mark, measure, panel_pad, pill, pill_radius,
        popup_offset, pressed_fill, pressed_wash, radius_control, radius_dialog, radius_item,
        radius_surface, row_height, spacing, strong_weight, target_outset, thumb_on,
    };
}
telar::__previews! {
    mod previews;

    /// Every preview this crate carries, its `.rsx` ones first: the one fn an application collects a library's previews through.
    pub fn telar_all_previews() -> Vec<telar::preview::PreviewEntry> {
        [telar_rsx_previews(), previews::previews()].concat()
    }
}
