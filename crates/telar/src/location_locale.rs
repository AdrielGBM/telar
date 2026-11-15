//! Binds the active locale to the locale the app's address carries.
//!
//! In the facade for the same reason as `system_locale`: `i18n-core` stays clear of the platform, and `platform-core` knows nothing of translations.

use std::rc::Rc;

use i18n_core::LocaleTag;

use crate::user_preferences::{LOCALE_KEY, stored_locale};

type Followed = Option<(reactive_core::OwnerId, Rc<Binding>)>;

reactive_core::dropless_thread_local! {
    // An effect stops only with its owner, so it gets a scope of its own that a re-call disposes; otherwise two bindings would race to write the address.
    static FOLLOW: released Followed = const { None };
}

/// Makes the locale part of where the user is: every location the app's address holds names one of `available`, and the active locale and that one follow each other.
///
/// - **Opening.** The address the app opens at decides: `/en/projects` on the web, `--location /en/projects` on a desktop or a terminal, an `ACTION_VIEW` link on Android, the history a desktop or a terminal saved, the fixed location of a headless run. An address that names none is written in the first of: the locale already set, the one the person last chose (kept under [`LOCALE_KEY`]), and [`negotiate_locale`](crate::negotiate_locale) of the system's preferred locales against `available`, falling back to `base`.
/// - **Switching.** [`set_locale`](crate::set_locale) rewrites the entry shown in the new locale, keeping the page and the anchor, without adding one: a language is how a place is shown, so back still leaves the page rather than undoing the language. An anchor whose name follows the locale is renamed in the address with it. The locale chosen is kept for the next run.
/// - **Arriving.** A link to a [`Location`](crate::Location) that names a locale switches to it; back and forward return to entries in the locale the app is in.
///
/// Routes never see the locale, and links and anchors are written with it, so an `<a href>` is right before any code runs. Call once at app start, in place of an initial `set_locale` and of [`follow_system_locale`](crate::follow_system_locale), whose live re-negotiation would move the address under the reader. Re-calling replaces the previous binding. An app that never calls it has addresses with no locale in them.
pub fn follow_location_locale(available: impl IntoIterator<Item = LocaleTag>, base: LocaleTag) {
    let mut available: Vec<LocaleTag> = available.into_iter().collect();
    if !available.contains(&base) {
        available.push(base.clone());
    }
    let binding = Rc::new(Binding {
        available: available.clone(),
        base,
    });
    if let Some((previous, _)) = FOLLOW.with(|follow| follow.borrow_mut().take()) {
        reactive_core::dispose_owner(previous);
    }
    platform_core::bind_location_locale(available, binding.clone());
    let scope = reactive_core::detached(reactive_core::owner_scope);
    let followed = binding.clone();
    reactive_core::effect(move || {
        let Some(active) = i18n_core::use_locale() else {
            return;
        };
        let carried = followed.carried(&active);
        platform_core::set_location_locale(carried);
        services_core::store_preference(LOCALE_KEY, Some(&carried.to_string()));
    });
    FOLLOW.with(|follow| *follow.borrow_mut() = Some((scope.id(), binding)));
}

/// The locale an address falls back to, as the last [`follow_location_locale`] named it; `None` for an app whose addresses carry none.
#[cfg(all(feature = "prerender", not(target_arch = "wasm32")))]
pub(crate) fn location_base_locale() -> Option<LocaleTag> {
    FOLLOW.with(|follow| {
        follow
            .borrow()
            .as_ref()
            .map(|(_, binding)| binding.base.clone())
    })
}

struct Binding {
    available: Vec<LocaleTag>,
    base: LocaleTag,
}

impl Binding {
    /// The locale an address carries for `locale`: the one of `available` it negotiates to, `es` for `es-CL`, or `base` for one the app does not ship.
    fn carried(&self, locale: &LocaleTag) -> &LocaleTag {
        self.shipped(locale).unwrap_or(&self.base)
    }

    fn shipped(&self, locale: &LocaleTag) -> Option<&LocaleTag> {
        i18n_core::negotiate_locale(std::slice::from_ref(locale), &self.available)
    }
}

impl platform_core::LocaleFollower for Binding {
    fn choose(&self) -> LocaleTag {
        let chosen = i18n_core::current_locale()
            .into_iter()
            .chain(stored_locale())
            .find_map(|locale| self.shipped(&locale).cloned());
        chosen.unwrap_or_else(|| {
            let preferred = preferences_core::system_preferences().locales;
            i18n_core::negotiate_locale(&preferred, &self.available)
                .unwrap_or(&self.base)
                .clone()
        })
    }

    fn adopt(&self, locale: &LocaleTag) {
        if i18n_core::current_locale().as_ref() != Some(locale) {
            i18n_core::set_locale(locale.clone());
        }
    }
}

#[cfg(test)]
#[path = "location_locale.test.rs"]
mod tests;
