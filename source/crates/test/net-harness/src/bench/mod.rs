//! The benches whose fixtures are the network harness's: the client's and the server's frames,
//! and the client's rollback check.

use criterion::Criterion;

mod client_frame;
mod rollback_scan;
mod server_frame;

/// Runs each bench of the crate whose id criterion's filter takes.
pub fn run(c: &mut Criterion) {
    client_frame::client_frame(c);
    rollback_scan::rollback_scan(c);
    server_frame::server_frame(c);
}
