//! First-party catalogue of high-level, fully-customizable widgets built on the rsx kernel primitives, opt-in via the `components` feature. Keep sigs in `telar-transpiler`'s `external_component_sigs()` in sync.

mod accordion;
mod badge;
mod button;
mod checkbox;
mod chip;
mod context_menu;
mod drawer;
mod dropdown;
#[cfg(test)]
mod harness;
mod heading;
mod list;
mod menu;
mod modal;
mod progress;
mod radio;
mod reorderable;
mod scrim;
mod section;
mod select;
mod shared;
mod slider;
mod spinner;
mod stepper;
mod tabs;
#[cfg(test)]
mod test_support;
mod text_field;
mod toggle;
mod tooltip;
mod window_frame;

pub use accordion::{AccordionProps, accordion};
pub use badge::{BadgeProps, badge};
pub use button::{ButtonProps, button};
pub use checkbox::{CheckboxProps, checkbox};
pub use chip::{ChipProps, chip};
pub use context_menu::{
    ContextMenuProps, Entry as MenuEntry, MenuCustomProps, MenuRowProps, MenuSeparatorProps,
    MenuStyle, MenuSubProps, context_menu, menu_custom, menu_row, menu_separator, menu_sub,
};
pub use drawer::{DrawerProps, drawer};
pub use heading::{HeadingProps, heading};
pub use list::{GroupProps, ItemProps, SeparatorProps, group, item, separator};
pub use menu::{MenuProps, menu};
pub use modal::{ModalProps, modal};
pub use progress::{ProgressProps, progress};
pub use radio::{RadioProps, radio};
pub use reorderable::{ReorderableProps, reorderable};
pub use section::{SectionProps, section};
pub use select::{SelectProps, select};
pub use slider::{SliderProps, slider};
pub use spinner::{SpinnerProps, spinner};
pub use stepper::{StepperProps, stepper};
pub use tabs::{TabsProps, tabs};
pub use text_field::{TextFieldProps, text_field};
pub use toggle::{ToggleProps, toggle};
pub use tooltip::{TooltipProps, tooltip};
pub use window_frame::{MIN_FRAME_SIZE, SurfaceFrameStyle, WindowControls, window_frame};
