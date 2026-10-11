use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use campfire_store::Worker;

use crate::gateway::MESSAGE_MAX;

/// A gateway on a loopback port: it answers each request it reads with the datagrams `answer`
/// gives, none for silence, and keeps each request.
pub(crate) struct FakeGateway {
    pub(crate) address: SocketAddrV4,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
    stop: Arc<AtomicBool>,
    _worker: Worker,
}

impl FakeGateway {
    pub(crate) fn start(
        mut answer: impl FnMut(&[u8]) -> Vec<Vec<u8>> + Send + 'static,
    ) -> FakeGateway {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_millis(5)))
            .unwrap();
        let SocketAddr::V4(address) = socket.local_addr().unwrap() else {
            unreachable!()
        };
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = {
            let (requests, stop) = (Arc::clone(&requests), Arc::clone(&stop));
            Worker::start("fake-gateway", move || {
                let mut buffer = [0; MESSAGE_MAX];
                while !stop.load(Ordering::Relaxed) {
                    let Ok((len, from)) = socket.recv_from(&mut buffer) else {
                        continue;
                    };
                    // Kept before the replies go, so a client that has its answer finds its request.
                    let replies = answer(&buffer[..len]);
                    requests.lock().unwrap().push(buffer[..len].to_vec());
                    for reply in replies {
                        socket.send_to(&reply, from).unwrap();
                    }
                }
            })
        };
        FakeGateway {
            address,
            requests,
            stop,
            _worker: worker,
        }
    }

    /// A PCP reply to `request`: its nonce, protocol and internal port, with `result`, a lifetime of
    /// the request's, and `external` as granted.
    pub(crate) fn pcp_reply(request: &[u8], result: u8, external: SocketAddrV4) -> Vec<u8> {
        let mut reply = request.to_vec();
        reply[1] = 0x81;
        reply[2] = 0;
        reply[3] = result;
        reply[8..24].fill(0);
        reply[8..12].copy_from_slice(&1000_u32.to_be_bytes());
        reply[42..44].copy_from_slice(&external.port().to_be_bytes());
        reply[44..60].copy_from_slice(&external.ip().to_ipv6_mapped().octets());
        reply
    }

    pub(crate) fn requests(&self) -> Vec<Vec<u8>> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for FakeGateway {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
