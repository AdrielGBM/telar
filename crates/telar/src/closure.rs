//! What a `.rsx` prop value says about a closure it hands to a callee the transpiler cannot see into: kept by it to call [`later`], or called [`now`].

/// Marks `f` as kept by the callee it is handed to and called later, so the component prop holding it is handed over as it is.
#[inline]
pub fn later<F>(f: F) -> F {
    f
}

/// Marks `f` as called on the spot by the callee it is handed to, so the component prop holding it follows what `f` reads.
#[inline]
pub fn now<F>(f: F) -> F {
    f
}

#[cfg(test)]
#[path = "closure_test.rs"]
mod tests;
