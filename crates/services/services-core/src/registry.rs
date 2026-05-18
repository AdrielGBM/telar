use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ServiceError {
    #[error("service already registered for this type")]
    AlreadyRegistered,
}

/// A registry for storing and retrieving typed services. Services are stored without `Send` bounds, making them compatible with the single-threaded reactive runtime. Services must only be accessed from the thread they were registered on.
#[derive(Default)]
pub struct ServiceRegistry {
    services: HashMap<TypeId, Box<dyn Any>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: Any + 'static>(&mut self, value: T) -> Result<(), ServiceError> {
        if self.services.contains_key(&TypeId::of::<T>()) {
            return Err(ServiceError::AlreadyRegistered);
        }
        self.services.insert(TypeId::of::<T>(), Box::new(value));
        Ok(())
    }

    pub fn get<T: Any + 'static>(&self) -> Option<&T> {
        self.services
            .get(&TypeId::of::<T>())
            .and_then(|any| any.downcast_ref())
    }

    pub fn get_mut<T: Any + 'static>(&mut self) -> Option<&mut T> {
        self.services
            .get_mut(&TypeId::of::<T>())
            .and_then(|any| any.downcast_mut())
    }

    pub fn remove<T: Any + 'static>(&mut self) -> Option<T> {
        self.services
            .remove(&TypeId::of::<T>())
            .and_then(|any| any.downcast().ok())
            .map(|boxed| *boxed)
    }

    pub fn contains<T: Any + 'static>(&self) -> bool {
        self.services.contains_key(&TypeId::of::<T>())
    }
}
