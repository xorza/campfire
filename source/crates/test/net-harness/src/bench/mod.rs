//! The benches whose fixtures are the network harness's: the client's and the server's frames.

use criterion::Criterion;

mod client_frame;
mod server_frame;

/// Runs each bench of the crate whose id criterion's filter takes.
pub fn run(c: &mut Criterion) {
    client_frame::client_frame(c);
    server_frame::server_frame(c);
}
