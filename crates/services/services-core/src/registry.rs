use std::any::{Any, TypeId};
use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub enum ServiceError {
    AlreadyRegistered,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyRegistered => write!(f, "service already registered for this type"),
        }
    }
}

impl std::error::Error for ServiceError {}

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
