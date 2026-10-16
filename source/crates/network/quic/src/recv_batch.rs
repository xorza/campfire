use std::io::{self, IoSliceMut};
use std::net::{IpAddr, SocketAddr};
use std::{array, iter};

use bytes::BytesMut;
use quinn_proto::EcnCodepoint;
use quinn_udp::{BATCH_SIZE, RecvMeta};

use crate::arena_ring::ArenaRing;
use crate::udp_port::UdpPort;

/// The thread's receive buffer: one read of the socket, `quinn-udp`'s batch, each slot as large as
/// the most datagrams GRO joins, and the arenas its datagrams are copied into.
#[derive(Debug)]
pub(crate) struct RecvBatch {
    buffer: Box<[u8]>,
    meta: [RecvMeta; BATCH_SIZE],
    slot: usize,
    arenas: ArenaRing,
}

/// One UDP datagram the socket read, in an arena with the others of its slot, which `quinn-proto`
/// takes and a DATAGRAM frame of it then keeps with no copy.
#[derive(Debug)]
pub(crate) struct Datagram {
    pub(crate) remote: SocketAddr,
    pub(crate) local_ip: Option<IpAddr>,
    pub(crate) ecn: Option<EcnCodepoint>,
    pub(crate) data: BytesMut,
}

impl RecvBatch {
    /// A batch for `port`, whose datagrams are at most `max_payload` bytes.
    pub(crate) fn new(port: &UdpPort, max_payload: usize) -> Self {
        Self::with_slot(max_payload * port.gro_segments())
    }

    fn with_slot(slot: usize) -> Self {
        Self {
            buffer: vec![0; slot * BATCH_SIZE].into_boxed_slice(),
            meta: [RecvMeta::default(); BATCH_SIZE],
            slot,
            arenas: ArenaRing::new(slot),
        }
    }

    /// Reads one batch of `port`: how many slots it filled, or `WouldBlock`.
    pub(crate) fn read(&mut self, port: &UdpPort) -> io::Result<usize> {
        let mut chunks = self.buffer.chunks_mut(self.slot).map(IoSliceMut::new);
        let mut slices: [IoSliceMut<'_>; BATCH_SIZE] =
            array::from_fn(|_| chunks.next().expect("the buffer holds BATCH_SIZE slots"));
        port.recv(&mut slices, &mut self.meta)
    }

    /// The datagrams of the `filled` slots the last read filled, each slot copied into an arena and
    /// split there into the datagrams GRO joined in it. An empty datagram, whose stride is 0,
    /// gives none.
    pub(crate) fn datagrams(&mut self, filled: usize) -> impl Iterator<Item = Datagram> + '_ {
        let Self {
            buffer,
            meta,
            slot,
            arenas,
        } = self;
        meta[..filled]
            .iter()
            .zip(buffer.chunks(*slot))
            .flat_map(|(meta, slot)| {
                let mut data = arenas.copy(&slot[..meta.len]);
                iter::from_fn(move || {
                    (!data.is_empty()).then(|| Datagram {
                        remote: meta.addr,
                        local_ip: meta.dst_ip,
                        ecn: meta.ecn.map(Self::proto_ecn),
                        data: data.split_to(meta.stride.min(data.len())),
                    })
                })
            })
    }

    const fn proto_ecn(ecn: quinn_udp::EcnCodepoint) -> EcnCodepoint {
        match ecn {
            quinn_udp::EcnCodepoint::Ect0 => EcnCodepoint::Ect0,
            quinn_udp::EcnCodepoint::Ect1 => EcnCodepoint::Ect1,
            quinn_udp::EcnCodepoint::Ce => EcnCodepoint::Ce,
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::net::SocketAddr;

    use crate::recv_batch::RecvBatch;

    impl RecvBatch {
        /// A batch that reads no socket, for a fixture that fills it, each slot `slot` bytes.
        pub(crate) fn detached(slot: usize) -> Self {
            Self::with_slot(slot)
        }

        /// Fills slot `index` as a read does: `bytes` from `remote`, datagrams of `stride` bytes
        /// that GRO joined, the last one shorter at times.
        pub(crate) fn load(
            &mut self,
            index: usize,
            remote: SocketAddr,
            bytes: &[u8],
            stride: usize,
        ) {
            self.buffer[index * self.slot..(index + 1) * self.slot][..bytes.len()]
                .copy_from_slice(bytes);
            let meta = &mut self.meta[index];
            meta.addr = remote;
            meta.len = bytes.len();
            meta.stride = stride;
            meta.ecn = None;
            meta.dst_ip = None;
        }
    }
}
