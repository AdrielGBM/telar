//! The first-party widget catalogue for Telar: buttons, fields, selects, tabs, sliders, menus, modals, accordions and the rest, built on the kernel's primitives through the `telar` facade the way an application's own components are.
//!
//! An application depends on it beside `telar` and names the groups it draws. Nothing is on by default, and the widgets every interface uses are in every build:
//!
//! - `overlays`: menus, context menus, modals, drawers and tooltips, and the scrim behind them.
//! - `chrome`: the window frame a desktop application draws itself.
//! - `advanced`: reorderable lists, accordions and steppers.
//! - `workbench`: split panes, trees, toolbars, icon buttons and key caps, with the `icons` and `overlays` they build on.
//!
//! ```toml
//! # Cargo.toml
//! telar-components = { version = "0.2.1", features = ["overlays"] }
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

#[cfg(feature = "advanced")]
mod accordion;
mod badge;
mod button;
mod checkbox;
mod chip;
#[cfg(feature = "overlays")]
mod context_menu;
#[cfg(feature = "overlays")]
mod drawer;
mod dropdown;
mod edit;
mod handle;
#[cfg(test)]
mod harness;
mod heading;
#[cfg(feature = "workbench")]
mod icon_button;
#[cfg(feature = "workbench")]
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
#[cfg(feature = "overlays")]
mod scrim;
mod scrub_field;
mod section;
mod select;
mod shared;
mod slider;
mod spinner;
#[cfg(feature = "workbench")]
mod split_pane;
#[cfg(feature = "advanced")]
mod stepper;
mod strings;
mod swatches;
mod tabs;
#[cfg(test)]
mod test_support;
mod text_field;
mod toggle;
#[cfg(feature = "workbench")]
mod toolbar;
#[cfg(feature = "overlays")]
mod tooltip;
#[cfg(feature = "workbench")]
mod tree_view;
mod type_ahead;
#[cfg(feature = "chrome")]
mod window_frame;

#[cfg(feature = "advanced")]
pub use accordion::{AccordionProps, accordion};
pub use badge::{BadgeProps, badge};
pub use button::{ButtonProps, button};
pub use checkbox::{CheckboxProps, checkbox};
pub use chip::{ChipProps, chip};
#[cfg(feature = "overlays")]
pub use context_menu::{
    ContextMenuProps, Entry as MenuEntry, MenuCustomProps, MenuRowProps, MenuSeparatorProps,
    MenuStyle, MenuSubProps, context_menu, menu_custom, menu_row, menu_separator, menu_sub,
};
#[cfg(feature = "overlays")]
pub use drawer::{DrawerProps, drawer};
pub use handle::{HandleProps, ToPoint, ToValue, handle};
pub use heading::{HeadingProps, heading};
#[cfg(feature = "workbench")]
pub use icon_button::{IconButtonProps, icon_button};
#[cfg(feature = "workbench")]
pub use kbd::{KbdProps, kbd};
pub use line_gutter::LineGutter;
pub use list::{GroupProps, ItemProps, SeparatorProps, group, item, separator};
#[cfg(feature = "overlays")]
pub use menu::{MenuProps, menu};
#[cfg(feature = "overlays")]
pub use modal::{ModalProps, modal};
pub use progress::{ProgressProps, progress};
pub use radio::{RadioProps, radio};
#[cfg(feature = "advanced")]
pub use reorder_zones::{ReorderGroup, ReorderZoneProps, Slot, apply_zone_move};
#[cfg(feature = "advanced")]
pub use reorderable::{ItemBuilder, ReorderableProps, reorderable};
pub use scrub_field::{DOUBLE_CLICK, Format, Parse, ScrubFieldProps, scrub_field};
pub use section::{SectionProps, section};
pub use select::{SelectProps, select};
pub use slider::{SliderProps, slider};
pub use spinner::{SpinnerProps, spinner};
#[cfg(feature = "workbench")]
pub use split_pane::{SizedPane, SplitDirection, SplitPaneProps, split_pane};
#[cfg(feature = "advanced")]
pub use stepper::{StepperProps, stepper};
pub use swatches::{SwatchesProps, swatches};
pub use tabs::{TabsProps, tabs};
pub use text_field::{TextFieldProps, text_field};
pub use toggle::{ToggleProps, toggle};
#[cfg(feature = "workbench")]
pub use toolbar::{ToolbarProps, toolbar};
#[cfg(feature = "overlays")]
pub use tooltip::{TooltipProps, tooltip};
#[cfg(feature = "workbench")]
pub use tree_view::{SelectHandler, TrailingBuilder, TreeNode, TreeViewProps, tree_view};
#[cfg(feature = "chrome")]
pub use window_frame::{MIN_FRAME_SIZE, SurfaceFrameStyle, WindowControls, window_frame};
