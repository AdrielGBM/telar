//! First-party catalogue of high-level, fully-customizable widgets built on the rsx kernel primitives, opt-in via the `components` feature.

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
#[cfg(test)]
mod harness;
mod heading;
mod list;
#[cfg(feature = "overlays")]
mod menu;
#[cfg(feature = "overlays")]
mod modal;
mod progress;
mod radio;
#[cfg(feature = "advanced")]
mod reorderable;
#[cfg(feature = "overlays")]
mod scrim;
mod section;
mod select;
mod shared;
mod slider;
mod spinner;
#[cfg(feature = "advanced")]
mod stepper;
mod tabs;
#[cfg(test)]
mod test_support;
mod text_field;
mod toggle;
#[cfg(feature = "overlays")]
mod tooltip;
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
pub use heading::{HeadingProps, heading};
pub use list::{GroupProps, ItemProps, SeparatorProps, group, item, separator};
#[cfg(feature = "overlays")]
pub use menu::{MenuProps, menu};
#[cfg(feature = "overlays")]
pub use modal::{ModalProps, modal};
pub use progress::{ProgressProps, progress};
pub use radio::{RadioProps, radio};
#[cfg(feature = "advanced")]
pub use reorderable::{ReorderableProps, reorderable};
pub use section::{SectionProps, section};
pub use select::{SelectProps, select};
pub use slider::{SliderProps, slider};
pub use spinner::{SpinnerProps, spinner};
#[cfg(feature = "advanced")]
pub use stepper::{StepperProps, stepper};
pub use tabs::{TabsProps, tabs};
pub use text_field::{TextFieldProps, text_field};
pub use toggle::{ToggleProps, toggle};
#[cfg(feature = "overlays")]
pub use tooltip::{TooltipProps, tooltip};
#[cfg(feature = "chrome")]
pub use window_frame::{MIN_FRAME_SIZE, SurfaceFrameStyle, WindowControls, window_frame};
