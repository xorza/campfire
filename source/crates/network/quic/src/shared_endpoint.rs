use std::sync::{LockResult, Mutex, MutexGuard};

use crate::endpoint_state::EndpointState;
use crate::udp_port::UdpPort;

/// What an endpoint's thread and the app's frame share: the endpoint's state behind one mutex,
/// and the socket, which the thread alone reads and writes and the frame only wakes (design 15,
/// D3).
#[derive(Debug)]
pub(crate) struct SharedEndpoint {
    state: Mutex<EndpointState>,
    port: UdpPort,
}

impl SharedEndpoint {
    pub(crate) const fn new(state: EndpointState, port: UdpPort) -> Self {
        Self {
            state: Mutex::new(state),
            port,
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, EndpointState> {
        self.state
            .lock()
            .expect("no thread panics while it holds an endpoint")
    }

    /// The state, or an error once a thread panicked while it held it.
    pub(crate) fn try_lock(&self) -> LockResult<MutexGuard<'_, EndpointState>> {
        self.state.lock()
    }

    pub(crate) const fn port(&self) -> &UdpPort {
        &self.port
    }

    /// Wakes the thread, which writes what the frame queued at once.
    pub(crate) fn wake(&self) {
        self.port.wake();
    }
}
