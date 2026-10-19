use std::io::{self, IoSliceMut};
use std::{array, iter};

use campfire_net_bytes::Arena;
use campfire_net_packets::{Datagram, datagram_arena};
use quinn_udp::{BATCH_SIZE, RecvMeta};

use crate::udp_port::UdpPort;

/// The thread's receive buffer: one read of the socket, `quinn-udp`'s batch, each slot as large as
/// the most datagrams GRO joins, and the arenas its datagrams are copied into.
#[derive(Debug)]
pub(crate) struct RecvBatch {
    buffer: Box<[u8]>,
    meta: [RecvMeta; BATCH_SIZE],
    slot: usize,
    arenas: Arena,
}

impl RecvBatch {
    /// A batch for `port`, whose datagrams are at most `max_payload` bytes.
    pub(crate) fn new(port: &UdpPort, max_payload: usize) -> Self {
        let slot = max_payload * port.gro_segments();
        Self {
            buffer: vec![0; slot * BATCH_SIZE].into_boxed_slice(),
            meta: [RecvMeta::default(); BATCH_SIZE],
            slot,
            arenas: datagram_arena(slot),
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
                        from: meta.addr,
                        payload: data.split_to(meta.stride.min(data.len())),
                    })
                })
            })
    }
}
