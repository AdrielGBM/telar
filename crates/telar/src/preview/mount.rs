//! Mounting a preview so that a change to an arg it read at build builds it again.

use std::rc::Rc;

use crate::{BuildFailure, ErrorBoundary, LayoutError, LayoutItem, ReactiveList};

use super::PreviewCtx;

/// Mounts `build` against `ctx`, inside an [`ErrorBoundary`] that shows `fallback` when it fails, and mounts it afresh each time [`super::host::Args::remounts`] moves.
///
/// Each mount is a new boundary, so a preview that failed for one value of an arg is tried again for the next. What the args hold, live signals included, outlives every mount.
pub fn remounting(
    ctx: PreviewCtx,
    build: impl Fn(&PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    fallback: impl Fn(BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let args = ctx.args().clone();
    let ctx = Rc::new(ctx);
    let build = Rc::new(build);
    let fallback = Rc::new(fallback);
    let mounts = ReactiveList::new(
        move || vec![args.remounts()],
        |remounts: &u64| *remounts,
        move |_| {
            let (ctx, build, fallback) = (Rc::clone(&ctx), Rc::clone(&build), Rc::clone(&fallback));
            let boundary =
                ErrorBoundary::new(move || build(&ctx), move |failure| fallback(failure))?;
            Ok(Box::new(boundary) as Box<dyn LayoutItem>)
        },
        0.0,
    )?;
    Ok(Box::new(mounts))
}
