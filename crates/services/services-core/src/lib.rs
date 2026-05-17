mod registry;
mod scope;

pub use registry::{ServiceError, ServiceRegistry};
pub use scope::{Scope, inject, provide, try_inject, with_service};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn registry_insert_get() {
        let mut reg = ServiceRegistry::new();
        reg.insert(42u32).unwrap();
        assert_eq!(reg.get::<u32>(), Some(&42));
    }

    #[test]
    fn registry_get_mut() {
        let mut reg = ServiceRegistry::new();
        reg.insert(0u32).unwrap();
        *reg.get_mut::<u32>().unwrap() = 7;
        assert_eq!(reg.get::<u32>(), Some(&7));
    }

    #[test]
    fn registry_remove() {
        let mut reg = ServiceRegistry::new();
        reg.insert(String::from("hello")).unwrap();
        let v = reg.remove::<String>();
        assert_eq!(v, Some(String::from("hello")));
        assert!(!reg.contains::<String>());
    }

    #[test]
    fn registry_insert_duplicate_returns_error() {
        let mut reg = ServiceRegistry::new();
        assert!(reg.insert(1u32).is_ok());
        assert_eq!(reg.insert(2u32), Err(ServiceError::AlreadyRegistered));
        assert_eq!(reg.get::<u32>(), Some(&1));
    }

    #[test]
    fn registry_missing_type() {
        let reg = ServiceRegistry::new();
        assert_eq!(reg.get::<u32>(), None);
    }

    #[test]
    fn scope_provide_inject() {
        Scope::with(|| {
            provide(99u32).unwrap();
            assert_eq!(try_inject::<u32>(), Some(99));
            assert_eq!(inject::<u32>(), 99);
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
                assert_eq!(inject::<u32>(), 2);
            });
        });
    }

    #[test]
    fn scope_drop_restores_parent() {
        Scope::with(|| {
            provide(String::from("parent")).unwrap();
            Scope::with(|| {
                provide(String::from("child")).unwrap();
                assert_eq!(inject::<String>(), "child");
            });
            assert_eq!(inject::<String>(), "parent");
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
            assert_eq!(inject::<u32>(), 42);
        });
        // service no longer available after with() returns
        assert_eq!(try_inject::<u32>(), None);
    }

    #[test]
    fn scope_with_nested() {
        Scope::with(|| {
            provide(String::from("outer")).unwrap();
            Scope::with(|| {
                provide(String::from("inner")).unwrap();
                assert_eq!(inject::<String>(), "inner");
            });
            assert_eq!(inject::<String>(), "outer");
        });
    }
}
