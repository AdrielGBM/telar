use crate::runtime;

pub type OwnerId = usize;

pub fn create_owner() -> OwnerId {
    runtime::create_owner()
}

pub fn drop_owner(id: OwnerId) {
    runtime::drop_owner(id);
}

pub fn with_owner<R>(id: OwnerId, f: impl FnOnce() -> R) -> R {
    runtime::with_owner(id, f)
}
