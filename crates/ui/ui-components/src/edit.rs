use reactive_core::Transaction;

/// If `transaction` is already open (a drag or popover in progress), previews `next` on it instead of starting a new commit, so a concurrent transaction on the same signal rebases onto it.
pub(crate) fn write_through(transaction: Transaction<f32>, next: f32) {
    if transaction.is_open() {
        let _ = transaction.preview(|value| *value = next);
        return;
    }
    if transaction.signal().peek() == next {
        return;
    }
    if transaction.begin().is_ok() {
        let _ = transaction.preview(|value| *value = next);
        let _ = transaction.commit();
    } else {
        transaction.signal().set(next);
    }
}

pub(crate) fn snap(raw: f32, increment: f32, min: f32, max: f32) -> f32 {
    let snapped = if increment > 0.0 {
        (raw / increment).round() * increment
    } else {
        raw
    };
    snapped.clamp(min, max)
}
