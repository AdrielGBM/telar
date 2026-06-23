use std::any::{Any, TypeId};
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ServiceError {
    #[error("service already registered for this type")]
    AlreadyRegistered,
}

#[derive(Default)]
pub(crate) struct ServiceRegistry {
    services: FxHashMap<TypeId, Rc<dyn Any>>,
    // Tracks only types inserted into THIS scope so a child can shadow an inherited parent service without an AlreadyRegistered error.
    own_keys: FxHashSet<TypeId>,
}

impl ServiceRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert<T: Any + 'static>(&mut self, value: T) -> Result<(), ServiceError> {
        if self.own_keys.contains(&TypeId::of::<T>()) {
            return Err(ServiceError::AlreadyRegistered);
        }
        self.own_keys.insert(TypeId::of::<T>());
        self.services.insert(TypeId::of::<T>(), Rc::new(value));
        Ok(())
    }

    pub(crate) fn get<T: Any + 'static>(&self) -> Option<&T> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|rc| rc.as_ref().downcast_ref::<T>())
    }

    pub(crate) fn merge_from(&mut self, other: &ServiceRegistry) {
        for (key, val) in &other.services {
            self.services.entry(*key).or_insert_with(|| Rc::clone(val));
        }
    }
}
