//! Mounting a preview so that a change to an arg it read at build builds it again.

use std::rc::Rc;

use crate::{
    BuildFailure, ErrorBoundary, LayoutError, LayoutItem, LayoutStyle, ReactiveList,
    SegmentNodeInfo,
};

use super::PreviewCtx;

/// Mounts `build` against `ctx`, inside an [`ErrorBoundary`] that shows `fallback` when it fails, and mounts it afresh each time [`super::host::Args::remounts`] moves.
///
/// Each mount is a new boundary, so a preview that failed for one value of an arg is tried again for the next. What the args hold, live signals included, outlives every mount.
///
/// The mount fills the box its parent gives it, so a preview that fills its own parent — a fullscreen page, a shell — fills that box too. A parent that sizes it to its content, as a scrolling column does, gets the preview at its content size.
pub fn remounting(
    ctx: PreviewCtx,
    build: impl Fn(&PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    fallback: impl Fn(BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let args = ctx.args().clone();
    let ctx = Rc::new(ctx);
    let build = Rc::new(build);
    let fallback = Rc::new(fallback);
    let mounts = ReactiveList::with_style(
        filling(),
        move || vec![args.remounts()],
        |remounts: &u64| *remounts,
        move |_| {
            let (ctx, build, fallback) = (Rc::clone(&ctx), Rc::clone(&build), Rc::clone(&fallback));
            let boundary = ErrorBoundary::with_style(
                filling(),
                move || build(&ctx),
                move |failure| fallback(failure),
            )?;
            Ok(Box::new(boundary) as Box<dyn LayoutItem>)
        },
    )?;
    Ok(Box::new(mounts))
}

/// The part of a walk of a tree [`remounting`] mounted in that is the preview's own: what is under the boundary it catches the preview's failures in, a level up for every level the host put around it. That boundary is the first in the walk, since a parent comes before what it holds and nothing a host puts around a preview is one. A walk with no boundary is returned whole.
pub fn preview_nodes(walk: &[SegmentNodeInfo]) -> Vec<SegmentNodeInfo> {
    let Some(at) = walk
        .iter()
        .position(|node| node.name == ErrorBoundary::SEGMENT_NAME)
    else {
        return walk.to_vec();
    };
    let below = walk[at].depth + 1;
    walk[at + 1..]
        .iter()
        .take_while(|node| node.depth >= below)
        .map(|node| SegmentNodeInfo {
            depth: node.depth - below,
            ..node.clone()
        })
        .collect()
}

fn filling() -> LayoutStyle {
    LayoutStyle::new()
        .flex_column()
        .flex_grow(1.0)
        .min_width(0.0)
        .min_height(0.0)
}

#[cfg(test)]
#[path = "mount_test.rs"]
mod tests;
