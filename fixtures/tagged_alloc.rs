//! A global allocator that marks every block it hands out and aborts the process on being handed back a block without its mark. A host and a guest each install one with a mark of their own, so a block one side allocated and the other freed stops the process on the spot, where two plain allocators would corrupt a heap and fail somewhere else, or not at all.

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write;

/// The allocator, carrying the mark it writes in front of every block.
pub struct Tagged(pub u64);

const HEADER: usize = 16;

fn outer(layout: Layout) -> (Layout, usize) {
    let header = layout.align().max(HEADER);
    let outer = Layout::from_size_align(layout.size() + header, header)
        .expect("a block and its mark fit a layout");
    (outer, header)
}

unsafe impl GlobalAlloc for Tagged {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (outer, header) = outer(layout);
        let base = unsafe { System.alloc(outer) };
        if base.is_null() {
            return base;
        }
        let block = unsafe { base.add(header) };
        unsafe { block.cast::<u64>().sub(1).write(self.0) };
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        let mark = unsafe { block.cast::<u64>().sub(1).read() };
        if mark != self.0 {
            // Allocating here would reenter this allocator, so the report is a fixed string.
            let _ = std::io::stderr().write_all(
                b"a block one side of the guest boundary allocated was freed by the other\n",
            );
            std::process::abort();
        }
        let (outer, header) = outer(layout);
        unsafe { System.dealloc(block.sub(header), outer) };
    }
}
