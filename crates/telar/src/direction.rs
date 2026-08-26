//! Binds the layout writing direction to the active locale.
//!
//! Lives in the facade rather than in either crate it joins: `i18n-core` is deliberately dependency-light
//! (only `reactive-core`) and must not learn about layout, and `layout-core` resolves direction without
//! knowing translations exist. The facade is the one place that already has both.

use std::cell::RefCell;

use layout_core::Direction;

thread_local! {
    // Replaced on re-call, since a hot reload re-runs the app's setup. Nothing here has a destructor any more — an `Effect` is an id — so the slot registers no TLS destructor and dlclose stays safe without a `ManuallyDrop`.
    static FOLLOW: RefCell<Option<reactive_core::Effect>> = const { RefCell::new(None) };
}

/// Makes the writing direction follow the active locale, so switching to Arabic or Hebrew mirrors the layout
/// and switching back restores it — no rebuild, the existing nodes are re-resolved on the next layout pass.
///
/// Call once at app start, next to the app's initial [`set_locale`](crate::set_locale) call. Apps that drive
/// direction themselves (a preview harness forcing RTL, say) can skip it and call
/// [`set_direction`](crate::set_direction) instead.
pub fn follow_locale_direction() {
    let effect = reactive_core::effect(|| {
        let direction = i18n_core::use_locale()
            .as_deref()
            .map(Direction::for_locale)
            .unwrap_or_default();
        ui_core::set_direction(direction);
    });
    FOLLOW.with(|f| *f.borrow_mut() = Some(effect));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_to_an_rtl_locale_flips_the_direction_and_back() {
        follow_locale_direction();
        i18n_core::set_locale("ar");
        assert_eq!(ui_core::current_direction(), Direction::Rtl);
        i18n_core::set_locale("es");
        assert_eq!(ui_core::current_direction(), Direction::Ltr);
    }
}
