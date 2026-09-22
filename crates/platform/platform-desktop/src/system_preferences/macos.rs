//! macOS' answers: the accessibility display options on `NSWorkspace` and `NSLocale`'s preferred languages, plus the notifications that say one of them changed.

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
use objc2_foundation::{
    NSCurrentLocaleDidChangeNotification, NSLocale, NSNotification, NSNotificationCenter,
};

pub fn reduced_motion() -> Option<bool> {
    Some(NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion())
}

pub fn high_contrast() -> Option<bool> {
    Some(NSWorkspace::sharedWorkspace().accessibilityDisplayShouldIncreaseContrast())
}

/// Already BCP 47 (`es-CL`, `es-419`), in the order System Settings lists them.
pub fn locales() -> Vec<String> {
    NSLocale::preferredLanguages()
        .iter()
        .map(|language| language.to_string())
        .collect()
}

type Observer = (
    Retained<NSNotificationCenter>,
    Retained<ProtocolObject<dyn NSObjectProtocol>>,
);

thread_local! {
    static OBSERVERS: RefCell<Vec<Observer>> = const { RefCell::new(Vec::new()) };
}

/// Calls `on_change` whenever the accessibility display options or the current locale change, until the next call replaces it.
///
/// The display options are announced on `NSWorkspace`'s own notification centre, not the default one, and the locale on the default one.
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) {
    let block = RcBlock::new(move |_: NonNull<NSNotification>| on_change());
    let workspace = NSWorkspace::sharedWorkspace().notificationCenter();
    let default = NSNotificationCenter::defaultCenter();
    let observers = unsafe {
        [
            (
                workspace.clone(),
                workspace.addObserverForName_object_queue_usingBlock(
                    Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                    None,
                    None,
                    &block,
                ),
            ),
            (
                default.clone(),
                default.addObserverForName_object_queue_usingBlock(
                    Some(NSCurrentLocaleDidChangeNotification),
                    None,
                    None,
                    &block,
                ),
            ),
        ]
    };
    OBSERVERS.with(|slot| {
        let mut slot = slot.borrow_mut();
        for (center, token) in slot.drain(..) {
            unsafe { center.removeObserver(token.as_ref()) };
        }
        slot.extend(observers);
    });
}
