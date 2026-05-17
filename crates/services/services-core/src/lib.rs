mod registry;
mod scope;

pub use registry::ServiceRegistry;
pub use scope::{Scope, inject, provide, try_inject, with_service};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn registry_insert_get() {
        let mut reg = ServiceRegistry::new();
        reg.insert(42u32);
        assert_eq!(reg.get::<u32>(), Some(&42));
    }

    #[test]
    fn registry_get_mut() {
        let mut reg = ServiceRegistry::new();
        reg.insert(0u32);
        *reg.get_mut::<u32>().unwrap() = 7;
        assert_eq!(reg.get::<u32>(), Some(&7));
    }

    #[test]
    fn registry_remove() {
        let mut reg = ServiceRegistry::new();
        reg.insert(String::from("hello"));
        let v = reg.remove::<String>();
        assert_eq!(v, Some(String::from("hello")));
        assert!(!reg.contains::<String>());
    }

    #[test]
    fn registry_overwrite() {
        let mut reg = ServiceRegistry::new();
        reg.insert(1u32);
        reg.insert(2u32);
        assert_eq!(reg.get::<u32>(), Some(&2));
    }

    #[test]
    fn registry_missing_type() {
        let reg = ServiceRegistry::new();
        assert_eq!(reg.get::<u32>(), None);
    }

    #[test]
    fn scope_provide_inject() {
        let _scope = Scope::new();
        provide(99u32);
        assert_eq!(try_inject::<u32>(), Some(99));
        assert_eq!(inject::<u32>(), 99);
    }

    #[test]
    fn scope_child_inherits_parent() {
        let _parent = Scope::new();
        provide(String::from("from-parent"));

        let _child = Scope::new();
        assert_eq!(try_inject::<String>(), Some(String::from("from-parent")));
    }

    #[test]
    fn scope_child_shadows_parent() {
        let _parent = Scope::new();
        provide(1u32);

        let _child = Scope::new();
        provide(2u32);

        assert_eq!(inject::<u32>(), 2);
    }

    #[test]
    fn scope_drop_restores_parent() {
        let _parent = Scope::new();
        provide(String::from("parent"));

        {
            let _child = Scope::new();
            provide(String::from("child"));
            assert_eq!(inject::<String>(), "child");
        }

        assert_eq!(inject::<String>(), "parent");
    }

    #[test]
    fn with_service_non_clone() {
        let _scope = Scope::new();
        provide(Rc::new(RefCell::new(vec![1, 2, 3])));

        let len = with_service(|v: &Rc<RefCell<Vec<i32>>>| v.borrow().len());
        assert_eq!(len, Some(3));
    }

    #[test]
    fn try_inject_missing_returns_none() {
        let _scope = Scope::new();
        assert_eq!(try_inject::<u64>(), None);
    }
}
