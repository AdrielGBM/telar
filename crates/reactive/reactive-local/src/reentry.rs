//! Naming both sides when a thread-local world is reached while it is already borrowed.
//!
//! Every world in the framework — the reactive runtime, and each [`surface_local!`](crate::surface_local) slot — is a `RefCell` behind a thread-local. Reaching one while it is already borrowed kills the process with `already borrowed: BorrowMutError` and a backtrace through the runtime's own frames. The call that collided is somewhere in that backtrace; the call it collided *with* is not, and that one is what says what actually happened — almost always a write whose flush ran an effect that came back round to the same world.
//!
//! Keeping it costs nothing on the path that succeeds: `borrow_mut` *is* `try_borrow_mut` with an `expect`, so this replaces that `expect` rather than adding a check.

use std::panic::Location;

/// Aborts naming the two operations that collided and where each of them is.
///
/// `held` is where the borrow in flight was most recently taken. It is a best effort rather than a proof: nothing tracks releases, so what it reports is the last borrow to succeed — which is the live one in every collision except a pair of nested shared borrows, where it names the inner of the two.
#[cold]
#[doc(hidden)]
pub fn borrow_collision(
    world: &str,
    held: Option<&'static Location<'static>>,
    here: &Location<'static>,
) -> ! {
    match held {
        Some(held) => panic!(
            "`{world}` is already borrowed.\n  \
             wanted by:  {here}\n  \
             last taken: {held}\n\
             The outer call is still on the stack, so the inner one reached the same world through it — \
             most often by writing a signal, whose flush runs effects that read this world. Copy what you \
             need out of the borrow, let it go, and do the write afterwards."
        ),
        None => panic!("`{world}` is already borrowed, and {here} asked for it again."),
    }
}
