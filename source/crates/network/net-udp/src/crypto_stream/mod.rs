use std::collections::{BTreeMap, btree_map};

/// The most handshake data of one epoch a stream holds ahead of what it delivered, as a peer's
/// handshake is small and a larger gap is no handshake's.
const MAX_AHEAD: u32 = 64 * 1024;

/// What a piece held ahead costs the bound past its bytes: its entry in the map, so pieces of one
/// byte each fill the bound as their memory does, as a channel charges its entries
/// (11-net-stack/transport.md, What a channel holds past a packet is a copy, charged with its
/// entry).
const ENTRY_COST: usize = size_of::<(u32, Vec<u8>)>();

/// One key epoch's handshake data as it arrives, in pieces from their offsets, which it hands on
/// in order and once each.
#[derive(Debug, Default)]
pub(crate) struct CryptoStream {
    delivered: u32,
    ahead: BTreeMap<u32, Vec<u8>>,
    /// The bytes `ahead` holds, overlaps counted each time, and `ENTRY_COST` for each piece.
    held: usize,
}

impl CryptoStream {
    /// Takes `data` from `offset`, and appends to `out` the bytes that now follow what was
    /// delivered. A piece that ends past `MAX_AHEAD`, that would hold more than `MAX_AHEAD` bytes
    /// ahead with its entry, or that was delivered already, adds nothing.
    pub(crate) fn push(&mut self, offset: u32, data: &[u8], out: &mut Vec<u8>) {
        let Some(end) = u32::try_from(data.len())
            .ok()
            .and_then(|len| offset.checked_add(len))
        else {
            return;
        };
        let max_ahead = usize::try_from(MAX_AHEAD).expect("64 KiB fits usize");
        if end <= self.delivered
            || end > self.delivered.saturating_add(MAX_AHEAD)
            || self.held + ENTRY_COST + data.len() > max_ahead
        {
            return;
        }
        if let btree_map::Entry::Vacant(entry) = self.ahead.entry(offset) {
            entry.insert(data.to_vec());
            self.held += ENTRY_COST + data.len();
        }
        while let Some(entry) = self.ahead.first_entry() {
            let start = *entry.key();
            if start > self.delivered {
                break;
            }
            let piece = entry.remove();
            self.held -= ENTRY_COST + piece.len();
            let skip = usize::try_from(self.delivered - start).expect("a piece fits usize");
            if let Some(fresh) = piece.get(skip..) {
                out.extend_from_slice(fresh);
                self.delivered += u32::try_from(fresh.len()).expect("a piece fits u32");
            }
        }
    }
}

/// One key epoch's handshake data as it is sent: the offset of the next piece.
#[derive(Debug, Default)]
pub(crate) struct CryptoSend {
    offset: u32,
}

impl CryptoSend {
    /// Takes `len` bytes of data and returns their offset.
    pub(crate) fn take(&mut self, len: usize) -> u32 {
        let offset = self.offset;
        self.offset += u32::try_from(len).expect("a handshake fits u32");
        offset
    }
}

#[cfg(test)]
mod tests;
