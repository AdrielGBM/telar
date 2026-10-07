#[cfg(feature = "overlays")]
use telar::relayout_if_dirty;
use telar::{
    AvailableSpace, Children, ComponentList, DrawCommand, LayoutItem, LayoutStyle, compute_layout,
    new_container, signal,
};
#[cfg(feature = "overlays")]
use telar_components::{ModalProps, modal};
use telar_components::{SelectProps, select};

fn reset() {
    telar::install_default_text_metrics();
    telar::reset_layout_runtime();
}

fn drawn_text(tree: &ComponentList) -> Vec<String> {
    tree.commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

fn lay_out(item: Box<dyn LayoutItem>) -> ComponentList {
    let root = new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[item.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    ComponentList::new(item)
}

pub fn select_placeholder() -> Vec<String> {
    reset();
    let item = select(
        SelectProps::props().selected(signal(0u32)).build(),
        Children::default(),
    )
    .unwrap();
    drawn_text(&lay_out(item))
}

#[cfg(feature = "overlays")]
pub fn open_modal(props: ModalProps) -> Vec<String> {
    reset();
    let item = modal(props, Children::default()).unwrap();
    let tree = lay_out(item);
    relayout_if_dirty();
    drawn_text(&tree)
}
