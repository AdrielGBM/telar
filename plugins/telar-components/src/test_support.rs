//! Setup every catalogue test starts from.

/// A layout runtime with nothing in it and a measurer for the text these widgets contain.
///
/// The measurer is the half that is easy to forget: laying a control out asks how wide its label is, and outside a runner nobody has installed anything to answer.
pub(crate) fn fresh_layout_runtime() {
    telar::install_default_text_metrics();
    telar::reset_layout_runtime();
}
