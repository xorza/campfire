//! The benches whose fixtures are the harness's: the server's tick and stages, the match's build,
//! the script view's read, the client's and the server's frames, and the allocations of the hot
//! script calls.

use criterion::Criterion;

use campfire_measure::Allocations;

use crate::moba_3v3;

mod allocations;
mod client_frame;
mod match_build;
mod server_frame;
mod server_tick;

/// Runs each bench of the crate whose id criterion's filter takes.
pub fn run(c: &mut Criterion) {
    server_tick::server(c);
    match_build::build(c);
    moba_3v3::bench::script_view(c);
    client_frame::client_frame(c);
    server_frame::server_frame(c);
}

/// Runs each bench of the allocations target whose id criterion's filter takes, as its counting
/// allocator counts.
pub fn run_allocations(c: &mut Criterion<Allocations>) {
    allocations::allocations(c);
}
