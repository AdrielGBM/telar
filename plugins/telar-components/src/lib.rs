//! The first-party widget catalogue for Telar: buttons, fields, selects, tabs, sliders, menus, modals, accordions and the rest, built on the kernel's primitives through the `telar` facade the way an application's own components are.
//!
//! An application depends on it beside `telar` and names the groups it draws. Nothing is on by default, and the widgets every interface uses — buttons, text fields, text areas, search fields, links, images, selects, tabs, sliders and indicators — are in every build:
//!
//! - `overlays`: menus, context menus, modals, drawers, tooltips, the command palette and the key caps its rows show, toasts, and the scrim behind them.
//! - `chrome`: the window frame a desktop application draws itself, and the window controls and title-bar behaviour it is built from, for a title bar of the application's own.
//! - `advanced`: reorderable lists, accordions, steppers, the virtual lists and data tables that build only the rows on screen, and the `resize_handle` a table's columns are resized by.
//! - `workbench`: split panes, resizable boxes, trees, toolbars, icon buttons, a colour picker and a code view, with the `icons` and `overlays` they build on.
//! - `async-assets`: lets an [`image`] follow an `AssetLoader`, whose load state converts into what the image shows.
//!
//! ```toml
//! # Cargo.toml
//! telar-components = { version = "0.2.2", features = ["overlays"] }
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
#[cfg(test)]
mod consistency_test;
#[cfg(feature = "advanced")]
mod data_table;
#[cfg(feature = "overlays")]
mod drawer;
mod dropdown;
mod edit;
#[cfg(feature = "overlays")]
mod fuzzy;
mod handle;
mod heading;
#[cfg(feature = "workbench")]
mod icon_button;
mod image;
#[cfg(feature = "overlays")]
mod kbd;
mod line_gutter;
mod link;
mod list;
#[cfg(feature = "overlays")]
mod menu;
#[cfg(feature = "overlays")]
mod menu_panel;
#[cfg(feature = "overlays")]
mod modal;
mod popup;
mod progress;
mod radio;
#[cfg(feature = "advanced")]
mod reorder_zones;
#[cfg(feature = "advanced")]
mod reorderable;
#[cfg(any(feature = "advanced", feature = "workbench"))]
mod resizable;
#[cfg(any(feature = "chrome", feature = "advanced", feature = "workbench"))]
mod resize;
#[cfg(any(feature = "advanced", feature = "overlays", feature = "workbench"))]
mod rows;
#[cfg(feature = "overlays")]
mod scrim;
mod scrub_field;
mod search_field;
mod section;
mod select;
#[cfg(feature = "advanced")]
mod selection;
mod shared;
mod slider;
mod spinner;
#[cfg(feature = "workbench")]
mod split_pane;
#[cfg(feature = "advanced")]
mod stepper;
mod strings;
#[cfg(test)]
mod style_scan_test;
mod swatches;
mod tabs;
#[cfg(test)]
mod test_support;
mod text_field;
#[cfg(feature = "chrome")]
mod title_bar;
#[cfg(feature = "overlays")]
mod toast;
mod toggle;
#[cfg(feature = "workbench")]
mod toolbar;
#[cfg(feature = "overlays")]
mod tooltip;
#[cfg(feature = "workbench")]
mod tree_view;
mod type_ahead;
#[cfg(feature = "advanced")]
mod virtual_list;
#[cfg(feature = "chrome")]
mod window_edges;
#[cfg(feature = "chrome")]
mod window_frame;

#[cfg(feature = "advanced")]
pub use accordion::{AccordionProps, AccordionTokens, accordion};
pub use badge::{BadgeProps, BadgeTokens, badge};
pub use button::{ButtonProps, ButtonTokens, ButtonVariant, button};
pub use checkbox::{CheckboxProps, CheckboxTokens, checkbox};
pub use chip::{ChipProps, ChipTokens, chip};
#[cfg(feature = "workbench")]
pub use code_view::{
    CodeSpan, CodeViewProps, CodeViewTokens, CopyHandler, TokenKind, TokenStyler, code_view,
};
#[cfg(feature = "workbench")]
pub use color_picker::{ColorPickerProps, ColorPickerTokens, color_picker};
#[cfg(feature = "overlays")]
pub use command_palette::{
    Command, CommandPaletteProps, CommandPaletteTokens, RunHandler, command_palette,
};
#[cfg(feature = "advanced")]
pub use data_table::{
    CellAlign, CellBuilder, Column, ColumnWidth, ColumnWidths, Comparator, DataTableProps,
    DataTableTokens, SortHandler, TableRow, TableRowHandler, TableSort, data_table,
};
#[cfg(feature = "overlays")]
pub use drawer::{DrawerProps, DrawerTokens, drawer};
#[cfg(feature = "overlays")]
pub use fuzzy::{FuzzyMatch, fuzzy_match};
pub use handle::{HandleProps, HandleTokens, ToPoint, ToValue, handle};
pub use heading::{HeadingProps, heading};
#[cfg(feature = "workbench")]
pub use icon_button::{IconButtonProps, IconButtonTokens, icon_button};
pub use image::{ImageProps, ImageSource, ImageTokens, image};
#[cfg(feature = "overlays")]
pub use kbd::{KbdProps, KbdTokens, kbd};
pub use line_gutter::LineGutter;
pub use link::{LinkProps, LinkTarget, LinkTokens, link};
pub use list::{GroupProps, ItemProps, ListTokens, SeparatorProps, group, item, separator};
#[cfg(feature = "overlays")]
pub use menu::{
    ContextMenuProps, MenuProps, MenuTokens, MenuTriggerTokens, context_menu, focused_rect,
    is_context_menu_key, menu,
};
#[cfg(feature = "overlays")]
pub use menu_panel::{SubmenuProps, submenu};
#[cfg(feature = "overlays")]
pub use modal::{ModalProps, ModalTokens, modal};
pub use progress::{ProgressProps, ProgressTokens, progress};
pub use radio::{RadioProps, RadioTokens, radio};
#[cfg(feature = "advanced")]
pub use reorder_zones::{ReorderGroup, ReorderZoneProps, Slot, apply_zone_move};
#[cfg(feature = "advanced")]
pub use reorderable::{ItemBuilder, ReorderableProps, reorderable};
#[cfg(feature = "workbench")]
pub use resizable::{ResizableProps, ResizeEdges, resizable};
#[cfg(any(feature = "advanced", feature = "workbench"))]
pub use resizable::{ResizableTokens, ResizeEdge, ResizeGrowth, ResizeHandleProps, resize_handle};
pub use scrub_field::{Format, Parse, ScrubFieldProps, ScrubFieldTokens, scrub_field};
pub use search_field::{SearchFieldProps, SearchHandler, search_field};
pub use section::{SectionProps, section};
pub use select::{SelectProps, SelectTokens, select};
#[cfg(feature = "advanced")]
pub use selection::{SelectionHandler, SelectionMode};
pub use shared::DOUBLE_CLICK;
pub use slider::{SliderProps, SliderTokens, slider};
pub use spinner::{SpinnerProps, SpinnerTokens, spinner};
#[cfg(feature = "workbench")]
pub use split_pane::{
    SizedPane, SplitDirection, SplitPaneProps, SplitPaneTokens, SplitRegion, split_pane,
};
#[cfg(feature = "advanced")]
pub use stepper::{StepperProps, StepperTokens, stepper};
pub use swatches::{SwatchesProps, SwatchesTokens, swatches};
pub use tabs::{Tab, TabsProps, TabsTokens, tab_list, tabs};
pub use text_field::{FieldKeyHandler, TextFieldProps, TextFieldTokens, text_field};
#[cfg(feature = "chrome")]
pub use title_bar::{TitleBarOptions, WindowControlsProps, title_bar, window_controls};
#[cfg(feature = "overlays")]
pub use toast::{
    Toast, ToastAction, ToastId, ToastKind, ToastPlacement, ToastTokens, ToasterProps, Toasts,
    show_toast, toaster, toasts,
};
pub use toggle::{ToggleProps, ToggleTokens, toggle};
#[cfg(feature = "workbench")]
pub use toolbar::{ToolbarProps, toolbar};
#[cfg(feature = "overlays")]
pub use tooltip::{TooltipProps, TooltipTokens, tooltip};
#[cfg(feature = "workbench")]
pub use tree_view::{
    SelectHandler, TrailingBuilder, TreeNode, TreeViewProps, TreeViewTokens, tree_view,
};
#[cfg(feature = "advanced")]
pub use virtual_list::{ListRow, RowBuilder, RowHandler, VirtualListProps, virtual_list};
#[cfg(feature = "chrome")]
pub use window_edges::window_edges;
#[cfg(feature = "chrome")]
pub use window_frame::{MIN_FRAME_SIZE, SurfaceFrameStyle, WindowControls, window_frame};

/// The geometry, washes, type and marks every catalogue control is drawn from, for an application drawing controls of its own that have to sit beside the catalogue's.
///
/// Each one reads the theme in force — a role such as `radius_control` or `highlight_low`, never a literal — and every length is scaled by the control size in force (`set_control_size`, a surface's own, or `provide_control_size` for a subtree), so a control built from these matches the catalogue's at any theme and any density:
///
/// ```ignore
/// use telar_components::metrics;
///
/// let shell = LayoutStyle::new()
///     .min_height(metrics::control_height())
///     .padding_horizontal(metrics::control_pad_x());
/// let paint = |_r| RectStyle::default()
///     .with_border(Border::uniform(border(), metrics::border_width()))
///     .with_radius(BorderRadius::all(metrics::radius_control()));
/// ```
///
/// Read them inside the closure that builds the style, so a theme switch or a density change re-runs it.
pub mod metrics {
    pub use crate::shared::{
        MIN_TARGET, Mark, border_width, border_width_strong, control_height, control_pad_x,
        control_text, focus_ring, gap_lg, gap_md, gap_sm, gap_xl, gap_xs, held_wash, hover_wash,
        hovered_fill, icon_pad, icon_size, ink_on, mark, measure, panel_pad, pill, pill_radius,
        popup_offset, pressed_fill, pressed_wash, radius_control, radius_dialog, radius_item,
        radius_surface, row_height, spacing, strong_weight, thumb_on,
    };
}
telar::__previews! {
    mod previews;

    /// Every preview this crate carries, its `.rsx` ones first: the one fn an application collects a library's previews through.
    pub fn telar_all_previews() -> Vec<telar::preview::PreviewEntry> {
        [telar_rsx_previews(), previews::previews()].concat()
    }
}
