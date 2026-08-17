//! Setup every catalogue test starts from.

/// A layout runtime with nothing in it and a measurer for the text these widgets contain.
///
/// The measurer is the half that is easy to forget: laying a control out asks how wide its label is, and outside a
/// runner nobody has installed anything to answer.
pub(crate) fn fresh_layout_runtime() {
    renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
    ui_core::reset_layout_runtime();
}
