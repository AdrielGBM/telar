/// Refusing a component that says the same thing twice about its own subtree.
///
/// All that is left of what used to be a `ServiceRegistry` here: a map of `TypeId` to `Rc<dyn Any>`, plus a
/// second set tracking which of them this scope inserted itself so a child could shadow a parent without
/// being told it already had one. The owner tree answers both — the map is the owner's, and shadowing is
/// what a walk that stops at the nearest match already does.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ServiceError {
    #[error("service already registered for this type")]
    AlreadyRegistered,
}
