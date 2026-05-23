use std::any::{Any, TypeId};
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ServiceError {
    #[error("service already registered for this type")]
    AlreadyRegistered,
}

#[derive(Default)]
pub struct ServiceRegistry {
    services: FxHashMap<TypeId, Rc<dyn Any>>,
    own_keys: FxHashSet<TypeId>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: Any + 'static>(&mut self, value: T) -> Result<(), ServiceError> {
        if self.own_keys.contains(&TypeId::of::<T>()) {
            return Err(ServiceError::AlreadyRegistered);
        }
        self.own_keys.insert(TypeId::of::<T>());
        self.services.insert(TypeId::of::<T>(), Rc::new(value));
        Ok(())
    }

    pub fn get<T: Any + 'static>(&self) -> Option<&T> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|rc| rc.as_ref().downcast_ref::<T>())
    }

    pub fn get_mut<T: Any + 'static>(&mut self) -> Option<&mut T> {
        self.services
            .get_mut(&TypeId::of::<T>())
            .and_then(|rc| Rc::get_mut(rc))
            .and_then(|any| any.downcast_mut::<T>())
    }

    pub fn remove<T: Any + 'static>(&mut self) -> Option<T> {
        self.own_keys.remove(&TypeId::of::<T>());
        let rc = self.services.remove(&TypeId::of::<T>())?;
        match Rc::downcast::<T>(rc) {
            Ok(rc_t) => match Rc::try_unwrap(rc_t) {
                Ok(val) => Some(val),
                Err(rc_t) => {
                    self.services.insert(TypeId::of::<T>(), rc_t);
                    None
                }
            },
            Err(rc) => {
                self.services.insert(TypeId::of::<T>(), rc);
                None
            }
        }
    }

    pub fn contains<T: Any + 'static>(&self) -> bool {
        self.services.contains_key(&TypeId::of::<T>())
    }

    pub fn merge_from(&mut self, other: &ServiceRegistry) {
        for (key, val) in &other.services {
            self.services.entry(*key).or_insert_with(|| Rc::clone(val));
        }
    }
}
