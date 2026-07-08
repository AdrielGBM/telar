//! First-party catalogue of high-level, fully-customizable widgets built on the rsx kernel primitives
//! (`box`/`text`/`on_press`/`hover`/`input`/`on_drag`/`overlay`). These are NOT part of the base language:
//! they resolve as ordinary component calls and are opt-in via the `rsx` crate's `components` feature, so an
//! app can drop them or ship its own. Third-party widget libraries follow the same shape (a `fn` + `Props`
//! per component). Keep the sigs in `rsx-transpiler`'s `external_component_sigs()` in sync with these.

mod accordion;
mod badge;
mod button;
mod checkbox;
mod chip;
mod drawer;
mod dropdown;
mod heading;
mod menu;
mod modal;
mod progress;
mod radio;
mod scrim;
mod section;
mod select;
mod shared;
mod slider;
mod spinner;
mod stepper;
mod tabs;
mod text_field;
mod toggle;
mod tooltip;

pub use accordion::{AccordionProps, accordion};
pub use badge::{BadgeProps, badge};
pub use button::{ButtonProps, button};
pub use checkbox::{CheckboxProps, checkbox};
pub use chip::{ChipProps, chip};
pub use drawer::{DrawerProps, drawer};
pub use heading::{HeadingProps, heading};
pub use menu::{MenuProps, menu};
pub use modal::{ModalProps, modal};
pub use progress::{ProgressProps, progress};
pub use radio::{RadioProps, radio};
pub use section::{SectionProps, section};
pub use select::{SelectProps, select};
pub use slider::{SliderProps, slider};
pub use spinner::{SpinnerProps, spinner};
pub use stepper::{StepperProps, stepper};
pub use tabs::{TabsProps, tabs};
pub use text_field::{TextFieldProps, text_field};
pub use toggle::{ToggleProps, toggle};
pub use tooltip::{TooltipProps, tooltip};
