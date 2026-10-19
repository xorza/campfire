//! The OS's UDP sockets of the stack's own protocol: a socket read and written in batches through
//! `quinn-udp`, with GSO and GRO, and waited on by its endpoint's IO thread, which carries a
//! server or a client of `net-udp` (13-net-stack/layering.md, The UDP protocol is a problem crate,
//! and its socket an IO crate).

mod bind_error;
mod client_socket;
mod recv_batch;
mod server_socket;
mod udp_io;
mod udp_port;

pub use crate::bind_error::BindError;
pub use crate::client_socket::ClientSocket;
pub use crate::server_socket::ServerSocket;
pub use crate::udp_io::UdpIo;
