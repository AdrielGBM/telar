//! Per-scope dependency injection: what `provide` records and `inject` finds.

/// Refusing a component that says the same thing twice about its own subtree.
///
/// The values themselves live on the owner tree, and a child shadows a parent because lookup stops at the nearest match, so the only thing left to refuse is a second `provide` of one type on one owner.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ServiceError {
    #[error("service already registered for this type")]
    AlreadyRegistered,
}
