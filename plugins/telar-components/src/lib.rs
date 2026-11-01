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

mod accordion;
mod button;
mod checkbox;
mod code_view;
mod color_picker;
mod command_palette;
#[cfg(test)]
mod consistency_test;
mod data_table;
mod drawer;
mod dropdown;
mod edit;
mod fuzzy;
mod handle;
#[cfg(test)]
mod heading_test;
mod icon_button;
mod image;
mod kbd;
mod line_gutter;
mod link;
mod list;
mod look;
mod menu;
mod menu_panel;
mod modal;
mod popup;
mod progress;
mod radio;
mod reorder_zones;
mod reorderable;
mod resizable;
mod resize;
mod rows;
mod scrim;
mod scrub_field;
mod search_field;
mod section;
mod select;
mod selection;
mod shared;
mod slider;
mod spinner;
mod split_pane;
mod stepper;
mod strings;
#[cfg(test)]
mod style_scan_test;
mod swatches;
mod tabs;
#[cfg(test)]
mod tag_test;
#[cfg(test)]
mod test_support;
mod text_field;
mod title_bar;
mod toast;
mod toggle;
mod toolbar;
mod tooltip;
mod tree_view;
mod type_ahead;
mod virtual_list;
mod window_edges;
mod window_frame;

pub use accordion::{AccordionProps, AccordionTokens, accordion};
pub use button::{ButtonProps, ButtonTokens, button};
pub use checkbox::{CheckboxProps, CheckboxTokens, checkbox};
pub use code_view::{
    CodeSpan, CodeViewProps, CodeViewTokens, CopyHandler, TokenKind, TokenStyler, code_view,
};
pub use color_picker::{ColorPickerProps, ColorPickerTokens, color_picker};
pub use command_palette::{
    Command, CommandPaletteProps, CommandPaletteTokens, RunHandler, command_palette,
};
pub use data_table::{
    CellAlign, CellBuilder, Column, ColumnWidth, ColumnWidths, Comparator, DataTableProps,
    DataTableTokens, SortHandler, TableRow, TableRowHandler, TableSort, data_table,
};
pub use drawer::{DrawerProps, DrawerTokens, drawer};
pub use fuzzy::{FuzzyMatch, fuzzy_match};
pub use handle::{HandleProps, HandleTokens, ToPoint, ToValue, handle};
pub use heading::{HeadingProps, heading};
pub use icon_button::{IconButtonProps, IconButtonTokens, icon_button};
pub use image::{ImageProps, ImageSource, ImageTokens, image};
pub use kbd::{KbdProps, KbdTokens, kbd};
pub use line_gutter::LineGutter;
pub use link::{LinkProps, LinkTarget, LinkTokens, link};
pub use list::{GroupProps, ItemProps, ListTokens, SeparatorProps, group, item, separator};
pub use menu::{
    ContextMenuProps, MenuProps, MenuTokens, MenuTriggerTokens, context_menu, focused_rect,
    is_context_menu_key, menu,
};
pub use menu_panel::{SubmenuProps, submenu};
pub use modal::{ModalProps, ModalTokens, modal};
pub use progress::{ProgressProps, ProgressTokens, progress};
pub use radio::{RadioProps, RadioTokens, radio};
pub use reorder_zones::{ReorderGroup, ReorderZoneProps, Slot, apply_zone_move};
pub use reorderable::{ItemBuilder, ReorderableProps, reorderable};
pub use resizable::{ResizableProps, ResizeEdges, resizable};
pub use resizable::{ResizableTokens, ResizeEdge, ResizeGrowth, ResizeHandleProps, resize_handle};
pub use scrub_field::{Format, Parse, ScrubFieldProps, ScrubFieldTokens, scrub_field};
pub use search_field::{SearchFieldProps, SearchHandler, search_field};
pub use section::{SectionProps, section};
pub use select::{SelectProps, SelectTokens, select};
pub use selection::{SelectionHandler, SelectionMode};
pub use shared::DOUBLE_CLICK;
pub use slider::{SliderProps, SliderTokens, slider};
pub use spinner::{SpinnerProps, SpinnerTokens, spinner};
pub use split_pane::{
    SizedPane, SplitDirection, SplitPaneProps, SplitPaneTokens, SplitRegion, split_pane,
};
pub use stepper::{StepperProps, StepperTokens, stepper};
pub use swatches::{SwatchesProps, SwatchesTokens, swatches};
pub use tabs::{Tab, TabsProps, TabsTokens, tab_list, tabs};
pub use tag::{TagProps, TagTokens, tag};
pub use text_field::{FieldKeyHandler, TextFieldProps, TextFieldTokens, text_field};
pub use title_bar::{TitleBarOptions, WindowControlsProps, title_bar, window_controls};
pub use toast::{
    Toast, ToastAction, ToastId, ToastPlacement, ToastTokens, ToasterProps, Toasts, show_toast,
    toaster, toasts,
};
pub use toggle::{ToggleProps, ToggleTokens, toggle};
pub use toolbar::{ToolbarProps, toolbar};
pub use tooltip::{TooltipProps, TooltipTokens, tooltip};
pub use tree_view::{
    ContextHandler, SelectHandler, TrailingBuilder, TreeContext, TreeNode, TreeViewProps,
    TreeViewTokens, tree_view,
};
pub use virtual_list::{ListRow, RowBuilder, RowHandler, VirtualListProps, virtual_list};
pub use window_edges::window_edges;
pub use window_frame::{
    MIN_FRAME_SIZE, SurfaceFrameStyle, WindowControls, WindowFrameProps, window_frame,
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
