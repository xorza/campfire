use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use campfire_store::Worker;

use crate::gateway::MESSAGE_MAX;

/// A gateway on a loopback port: it answers each request it reads with the datagrams `answer`
/// gives, none for silence, and keeps each request. Its read blocks with no timeout, as a read
/// that times out on Windows may cancel as a datagram completes it and lose that datagram; the
/// drop sends it an empty datagram, which no client sends, to stop it.
pub(crate) struct FakeGateway {
    pub(crate) address: SocketAddrV4,
    requests: Arc<Requests>,
    _worker: Worker,
}

/// The requests a fake gateway kept, and the signal of each new one.
#[derive(Debug, Default)]
struct Requests {
    kept: Mutex<Vec<Vec<u8>>>,
    added: Condvar,
}

impl FakeGateway {
    pub(crate) fn start(mut answer: impl FnMut(&[u8]) -> Vec<Vec<u8>> + Send + 'static) -> Self {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let SocketAddr::V4(address) = socket.local_addr().unwrap() else {
            unreachable!()
        };
        let requests = Arc::new(Requests::default());
        let worker = {
            let requests = Arc::clone(&requests);
            Worker::start("fake-gateway", move || {
                let mut buffer = [0; MESSAGE_MAX];
                loop {
                    // A reply to a client gone reaches Windows as a reset on the next read.
                    let Ok((len, from)) = socket.recv_from(&mut buffer) else {
                        continue;
                    };
                    if len == 0 {
                        break;
                    }
                    // Kept before the replies go, so a client that has its answer finds its request.
                    let replies = answer(&buffer[..len]);
                    requests.kept.lock().unwrap().push(buffer[..len].to_vec());
                    requests.added.notify_all();
                    for reply in replies {
                        socket.send_to(&reply, from).unwrap();
                    }
                }
            })
        };
        Self {
            address,
            requests,
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
        self.requests.kept.lock().unwrap().clone()
    }

    /// The requests it kept, once it kept `count` or a second passed: it reads each on a thread
    /// of its own, which a loaded runner may run after the client stopped waiting for an answer.
    pub(crate) fn requests_once(&self, count: usize) -> Vec<Vec<u8>> {
        let kept = self.requests.kept.lock().unwrap();
        let (kept, _) = self
            .requests
            .added
            .wait_timeout_while(kept, Duration::from_secs(1), |kept| kept.len() < count)
            .unwrap();
        kept.clone()
    }
}

impl Drop for FakeGateway {
    fn drop(&mut self) {
        let waker = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        waker.send_to(&[], self.address).unwrap();
    }
}
