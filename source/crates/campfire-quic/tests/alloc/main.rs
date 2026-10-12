//! The QUIC transport's allocation audit (design 15, D3). `frame` measures the frame's two
//! systems over real endpoints, whose threads no window counts; `thread` drives a server's thread
//! a pass at a time on the test's own thread, so its windows count what the thread allocates.

use campfire_common::internals::CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

mod frame;
mod loopback;
mod thread;
