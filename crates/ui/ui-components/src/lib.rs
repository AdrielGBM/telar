//! First-party catalogue of high-level, fully-customizable widgets built on the rsx kernel primitives
//! (`box`/`text`/`on_press`/`hover`/`input`/`on_drag`/`overlay`). These are NOT part of the base language:
//! they resolve as ordinary component calls and are opt-in via the `rsx` crate's `components` feature, so an
//! app can drop them or ship its own. Third-party widget libraries follow the same shape (a `fn` + `Props`
//! per component). Keep the sigs in `rsx-transpiler`'s `external_component_sigs()` in sync with these.

mod button;
mod checkbox;
mod drawer;
mod heading;
mod menu;
mod modal;
mod radio;
mod section;
mod select;
mod slider;
mod text_field;
mod toggle;
mod tooltip;

pub use button::{ButtonProps, button};
pub use checkbox::{CheckboxProps, checkbox};
pub use drawer::{DrawerProps, drawer};
pub use heading::{HeadingProps, heading};
pub use menu::{MenuProps, menu};
pub use modal::{ModalProps, modal};
pub use radio::{RadioProps, radio};
pub use section::{SectionProps, section};
pub use select::{SelectProps, select};
pub use slider::{SliderProps, slider};
pub use text_field::{TextFieldProps, text_field};
pub use toggle::{ToggleProps, toggle};
pub use tooltip::{TooltipProps, tooltip};
