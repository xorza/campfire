//! The QUIC transport's allocation audit (11-quic-transport.md, A thread for each endpoint, and no
//! IO in the frame). `frame` measures the frame's two systems over real endpoints, their threads
//! among them; `thread` drives a server's thread a pass at a time on the test's own thread, so a
//! window of a pass counts the pass alone.

use campfire_measure::CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

mod frame;
mod loopback;
mod thread;
