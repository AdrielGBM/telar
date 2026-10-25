//! The byte spelling of the component tree an inspector reads, for a guest's tree to cross to a host's overlay.

use wire_core::wire_struct;

use crate::SegmentNodeInfo;

wire_struct!(SegmentNodeInfo {
    id,
    name,
    depth,
    rect,
    padding,
    margin,
    border,
    gap
});

#[cfg(test)]
#[path = "wire_test.rs"]
mod tests;
