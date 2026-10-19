use std::collections::{BTreeMap, btree_map};

/// The most handshake data of one epoch a stream holds ahead of what it delivered, as a peer's
/// handshake is small and a larger gap is no handshake's.
const MAX_AHEAD: u32 = 64 * 1024;

/// One key epoch's handshake data as it arrives, in pieces from their offsets, which it hands on
/// in order and once each.
#[derive(Debug, Default)]
pub(crate) struct CryptoStream {
    delivered: u32,
    ahead: BTreeMap<u32, Vec<u8>>,
    /// The bytes `ahead` holds, overlaps counted each time.
    held: usize,
}

impl CryptoStream {
    /// Takes `data` from `offset`, and appends to `out` the bytes that now follow what was
    /// delivered. A piece that ends past `MAX_AHEAD`, that would hold more than `MAX_AHEAD` bytes
    /// ahead, or that was delivered already, adds nothing.
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
            || self.held + data.len() > max_ahead
        {
            return;
        }
        if let btree_map::Entry::Vacant(entry) = self.ahead.entry(offset) {
            entry.insert(data.to_vec());
            self.held += data.len();
        }
        while let Some(entry) = self.ahead.first_entry() {
            let start = *entry.key();
            if start > self.delivered {
                break;
            }
            let piece = entry.remove();
            self.held -= piece.len();
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
mod tests {
    use super::*;

    /// Pieces that arrive out of order, overlap, or come again hand on each byte once, in order:
    /// "lo" at 3 waits, "hel" at 0 releases "hello", "llo w" at 2 adds " w", and "he" again adds
    /// nothing.
    #[test]
    fn pieces_hand_on_each_byte_once_in_order() {
        let mut stream = CryptoStream::default();
        let mut out = Vec::new();
        stream.push(3, b"lo", &mut out);
        assert!(out.is_empty());
        stream.push(0, b"hel", &mut out);
        assert_eq!(out, b"hello");
        stream.push(2, b"llo w", &mut out);
        assert_eq!(out, b"hello w");
        stream.push(0, b"he", &mut out);
        assert_eq!(out, b"hello w");
        stream.push(MAX_AHEAD + 8, b"far", &mut out);
        assert_eq!(out, b"hello w");
        // From 7 delivered, a piece may end at 7 + 65,536 = 65,543 at most.
        stream.push(MAX_AHEAD + 6, b"ab", &mut out);
        stream.push(MAX_AHEAD + 5, b"ab", &mut out);
        assert_eq!(stream.held, 2);
        stream.held = 0;
        stream.ahead.clear();
        // Pieces of 1,000 bytes at 8, 9, 10, … overlap, and each counts whole: 65 hold 65,000
        // bytes, and the 66th would pass 65,536. The byte at 7 releases them all.
        let piece = [b'x'; 1_000];
        for offset in 8..8 + 66 {
            stream.push(offset, &piece, &mut out);
        }
        assert_eq!([stream.ahead.len(), stream.held], [65, 65_000]);
        stream.push(7, b"!", &mut out);
        assert_eq!([stream.ahead.len(), stream.held], [0, 0]);
        assert_eq!(out.len(), 8 + 1_000 + 64);
        let mut send = CryptoSend::default();
        assert_eq!([send.take(3), send.take(4), send.take(0)], [0, 3, 7]);
    }
}
