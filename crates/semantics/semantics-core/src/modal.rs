/// Said of each box a modal layer is made of: while it is drawn, the keyboard stays inside the layer and the rest of its surface is out of reach, for a reader as much as for Tab.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Modal {
    /// Names the layer, the same on every box it is made of: the element id of the first of them.
    pub layer: u64,
    /// The control the layer was opened from, by its element id, which stays within reach while the layer is up: a select's combobox keeps the keyboard while its list is open, and a menu button still says its menu is expanded.
    pub opened_from: Option<u64>,
}
