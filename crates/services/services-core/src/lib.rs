pub mod app_paths;
pub mod clipboard;
pub mod dialogs;
pub mod paths;
#[cfg(feature = "di")]
mod registry;
#[cfg(feature = "di")]
mod scope;

pub use clipboard::{Clipboard, clipboard, clipboard_text, set_clipboard, set_clipboard_text};
pub use dialogs::{FileDialog, FileDialogs, FileFilter, file_dialogs, set_file_dialogs};
#[cfg(feature = "system-paths")]
pub use paths::SystemPaths;
pub use paths::{AppPathsProvider, NoPaths};
#[cfg(feature = "di")]
pub use registry::ServiceError;
#[cfg(feature = "di")]
pub use scope::{Scope, context, provide, set_context, try_inject, with_service};

#[cfg(all(test, feature = "di"))]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn scope_provide_inject() {
        Scope::with(|| {
            provide(99u32).unwrap();
            assert_eq!(try_inject::<u32>(), Some(99));
        });
    }

    #[test]
    fn scope_child_inherits_parent() {
        Scope::with(|| {
            provide(String::from("from-parent")).unwrap();
            Scope::with(|| {
                assert_eq!(try_inject::<String>(), Some(String::from("from-parent")));
            });
        });
    }

    #[test]
    fn scope_child_shadows_parent() {
        Scope::with(|| {
            provide(1u32).unwrap();
            Scope::with(|| {
                provide(2u32).unwrap();
                assert_eq!(try_inject::<u32>(), Some(2));
            });
        });
    }

    #[test]
    fn scope_drop_restores_parent() {
        Scope::with(|| {
            provide(String::from("parent")).unwrap();
            Scope::with(|| {
                provide(String::from("child")).unwrap();
                assert_eq!(try_inject::<String>().as_deref(), Some("child"));
            });
            assert_eq!(try_inject::<String>().as_deref(), Some("parent"));
        });
    }

    #[test]
    fn with_service_non_clone() {
        Scope::with(|| {
            provide(Rc::new(RefCell::new(vec![1, 2, 3]))).unwrap();
            let len = with_service(|v: &Rc<RefCell<Vec<i32>>>| v.borrow().len());
            assert_eq!(len, Some(3));
        });
    }

    #[test]
    fn try_inject_missing_returns_none() {
        Scope::with(|| {
            assert_eq!(try_inject::<u64>(), None);
        });
    }

    #[test]
    fn scope_with_provides_service() {
        Scope::with(|| {
            provide(42u32).unwrap();
            assert_eq!(try_inject::<u32>(), Some(42));
        });
        assert_eq!(try_inject::<u32>(), None);
    }

    #[test]
    fn scope_with_cleans_up_after_closure_returns() {
        Scope::with(|| {
            provide(String::from("ephemeral")).unwrap();
            assert!(try_inject::<String>().is_some());
        });
        assert_eq!(
            try_inject::<String>(),
            None,
            "service must not be visible after Scope::with returns"
        );
    }

    #[test]
    fn scope_with_nested() {
        Scope::with(|| {
            provide(String::from("outer")).unwrap();
            Scope::with(|| {
                provide(String::from("inner")).unwrap();
                assert_eq!(try_inject::<String>().as_deref(), Some("inner"));
            });
            assert_eq!(try_inject::<String>().as_deref(), Some("outer"));
        });
    }
}
