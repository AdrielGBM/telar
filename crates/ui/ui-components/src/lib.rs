//! First-party catalogue of high-level, fully-customizable widgets built on the rsx kernel primitives
//! (`box`/`text`/`on_press`/`hover`). These are NOT part of the base language: they resolve as ordinary
//! component calls and are opt-in via the `rsx` crate's `widgets` feature, so an app can drop them or
//! ship its own. Third-party widget libraries follow the same shape (a `fn` + `Props` per component).

mod button;
mod heading;
mod section;

pub use button::{ButtonProps, button};
pub use heading::{HeadingProps, heading};
pub use section::{SectionProps, section};
