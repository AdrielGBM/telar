//! The application's vocabulary of named values, and the root of what flows down its tree.
//!
//! A theme is one of four layers that decide what a property is worth here, and knowing which is which is
//! most of knowing where to put something:
//!
//! | | Owns | Scope |
//! | --- | --- | --- |
//! | **`[style]`** | named classes | one `.rsx` file, resolved at transpile time, no runtime cost |
//! | **The theme** | the application's value vocabulary, and the root [`Inherited`] | the application, swappable at runtime |
//! | **`Inherited`** | which property has which value *here* | a subtree, resolved per node |
//! | **An attribute** | one property on one node | one node |
//!
//! Each overrides the one above it. The middle two are the ones that did not exist: for a long time there
//! was a theme and there was an attribute and nothing in between, which is why everything in between had to
//! be a global — a process-wide font family, a thread-local control size, a catalogue reading tokens behind
//! the markup's back and arriving at a different answer from the `text` beside it.
//!
//! What a theme is *not* is a second channel beside the cascade, and the trait is shaped to make that
//! unwritable: a property that *inherits* has no token here, only a place in [`ThemeTokens::root`]. So "the
//! body text is 11px" is a theme change said once at the top, and there is no reader a component could use to
//! reach past the region it is standing in. See `telar-ui-core`'s `inherit` module for how a declaration
//! reaches a node.
//!
//! [`Inherited`]: https://docs.rs/telar-ui-core/latest/telar_ui_core/struct.Inherited.html

mod context;
mod density;
mod mode;

pub use context::{Theme, ThemeTokens, set_theme, use_theme, use_theme_tokens};
pub use density::{ControlSize, control_scale, set_control_size, use_control_size};
pub use mode::{active_mode, follow_system, register_mode, set_mode, set_system_dark};
