//! Binds the layout writing direction to the active locale.
//!
//! `i18n-core` says which direction a locale is written in and `ui-core` holds the direction every surface lays out against; neither makes one follow the other, since an application may drive the direction itself. Binding them is the facade's opt-in.

use std::cell::Cell;

use i18n_core::Direction;

reactive_core::dropless_thread_local! {
    // An effect stops only with its owner, so the follower gets a scope of its own that a re-call disposes; a hot reload re-runs the app's setup.
    static FOLLOW: Cell<Option<reactive_core::OwnerId>> = const { Cell::new(None) };
}

/// Makes the writing direction follow the active locale, so switching to Arabic or Hebrew mirrors the layout and switching back restores it — no rebuild, the existing nodes are re-resolved on the next layout pass.
///
/// Call once at app start, next to the app's initial [`set_locale`](crate::set_locale) call. Re-calling replaces the previous follower. Apps that drive direction themselves (a preview harness forcing RTL, say) can skip it and call [`set_direction`](crate::set_direction) instead.
pub fn follow_locale_direction() {
    if let Some(previous) = FOLLOW.take() {
        reactive_core::dispose_owner(previous);
    }
    let scope = reactive_core::detached(reactive_core::owner_scope);
    reactive_core::effect(|| {
        let direction = i18n_core::use_locale()
            .as_deref()
            .map(Direction::for_locale)
            .unwrap_or_default();
        ui_core::set_direction(direction);
    });
    FOLLOW.set(Some(scope.id()));
}

#[cfg(test)]
#[path = "direction.test.rs"]
mod tests;
