mod component;
mod tree;
mod view;
mod view_flatten;

pub use component::{Component, EventResult};
pub use tree::{ComponentTree, SubtreeSlot};
pub use view::{IntoView, SubtreeHandle, View};
pub use view_flatten::flatten_view;
