//! The family a surface's text shapes in when nothing on its way down names one.
//!
//! Per surface, like its title: two windows in one process can show two faces, and a texture UI its pixel face inside a window that shows the platform's. It sits at the root of the surface's text cascade, under the theme's own root row, so every text that inherits — every `text` in `.rsx` — follows it as it changes, with nothing reopened and nothing rebuilt by hand.

use reactive_core::{RwSignal, in_surface_world, signal};
use renderer_core::FontFamily;

#[derive(Clone, Copy)]
struct DefaultFamily {
    family: RwSignal<Option<FontFamily>>,
    /// Whether the app named one itself, which a later opening of the surface keeps.
    named: bool,
}

reactive_core::surface_local! {
    /// The active surface's default family, created with the first call that needs it.
    slot DEFAULT_FAMILY: Option<DefaultFamily> = None;
    access with_default_family, with_default_family_ref;
    context SurfaceFontContext, SurfaceFontGuard;
}

fn existing() -> Option<DefaultFamily> {
    with_default_family_ref(|slot| *slot).filter(|slot| slot.family.is_alive())
}

fn default_family() -> DefaultFamily {
    if let Some(slot) = existing() {
        return slot;
    }
    // In the surface's own world, so the signal lives as long as the surface rather than as long as whichever scope asked first.
    let slot = DefaultFamily {
        family: in_surface_world(|| signal(None)),
        named: false,
    };
    with_default_family(|stored| *stored = Some(slot));
    slot
}

fn write(slot: DefaultFamily, family: Option<FontFamily>) {
    if slot.family.peek() != family {
        slot.family.set(family);
    }
}

/// Sets the family the active surface's text shapes in wherever nothing above it names one; `None` goes back to the platform's own.
///
/// Every text that inherits its style is laid out and drawn again in the new family on the next frame. The theme's root row and any `font_family:` declared above a text still win over it, as an author's stylesheet wins over a browser's default font. A family the system does not have falls back to the platform's sans-serif, which `telar::font_family_available` can warn about.
///
/// What the app sets here outlives the tree: a surface opened again on a new tree keeps it rather than going back to its configuration's.
pub fn set_font_family(family: Option<FontFamily>) {
    let slot = default_family();
    with_default_family(|stored| {
        *stored = Some(DefaultFamily {
            named: true,
            ..slot
        })
    });
    write(slot, family);
}

/// Reactive read of the active surface's default family, `None` while it shapes in the platform's own.
pub fn use_font_family() -> Option<FontFamily> {
    default_family().family.get()
}

/// Tells the active surface the family its configuration names — `AppConfig::font_family`. The runner calls this before it builds the surface's tree; a host or a test with no runner calls it to stand in for one.
///
/// It only seeds: once [`set_font_family`] has named one, a later opening keeps it.
pub fn open_surface_font_family(configured: Option<FontFamily>) {
    let slot = default_family();
    if !slot.named {
        write(slot, configured);
    }
}

/// The family the root of the active surface's cascade starts from: its default, falling back to the platform's sans-serif where the system lacks the face it names.
pub(crate) fn root_family() -> FontFamily {
    match use_font_family() {
        None => FontFamily::SansSerif,
        Some(family) => falling_back(family),
    }
}

fn falling_back(family: FontFamily) -> FontFamily {
    let last = match &family {
        FontFamily::Stack(members) => members.last().cloned(),
        other => Some(other.clone()),
    };
    if !matches!(last, Some(FontFamily::Named(_))) {
        return family;
    }
    let members = match family {
        FontFamily::Stack(members) => members.to_vec(),
        named => vec![named],
    };
    FontFamily::stack(members.into_iter().chain([FontFamily::SansSerif]))
}

#[cfg(test)]
#[path = "surface_font_test.rs"]
mod tests;
