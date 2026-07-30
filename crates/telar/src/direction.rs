//! Binds the layout writing direction to the active locale.
//!
//! Lives in the facade rather than in either crate it joins: `i18n-core` is deliberately dependency-light
//! (only `reactive-core`) and must not learn about layout, and `layout-core` resolves direction without
//! knowing translations exist. The facade is the one place that already has both.

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use layout_core::Direction;

thread_local! {
    // Keeps the effect alive for the app's lifetime; replaced on re-call, since a hot reload re-runs the app's setup. ManuallyDrop for the same dlclose-safety reason as theme-core's signals.
    static FOLLOW: ManuallyDrop<RefCell<Option<reactive_core::Effect>>> =
        ManuallyDrop::new(RefCell::new(None));
}

/// Makes the writing direction follow the active locale, so switching to Arabic or Hebrew mirrors the layout
/// and switching back restores it — no rebuild, the existing nodes are re-resolved on the next layout pass.
///
/// Call once at app start, next to [`init_locale`](crate::init_locale). Apps that drive direction themselves
/// (a preview harness forcing RTL, say) can skip it and call [`set_direction`](crate::set_direction) instead.
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
