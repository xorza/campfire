use blake3::Hasher;

use crate::journal::error::NotJournal;

/// Starts the hash of every frame, so no other BLAKE3 use can produce one.
const FRAME_DOMAIN: &[u8] = b"campfire/journal-frame/v1";
/// The longest record a frame holds: a longer length is a torn or corrupted frame.
pub(crate) const MAX_RECORD: usize = 16 << 20;
const LEN_BYTES: usize = size_of::<u32>();
const HASH_BYTES: usize = 32;

/// The whole frames of a journal's bytes, read in order: each a `u32` length, the record's bytes,
/// and the 32-byte BLAKE3 of `domain ‖ record`, as `LevelDB` frames its log with a hash in place of
/// its CRC32C. The read stops at the first frame that is short, longer than `MAX_RECORD`, or whose
/// hash fails: a crash tears at most the frames being written, and the journal is cut there.
#[derive(Debug)]
pub struct JournalFrames<'a> {
    bytes: &'a [u8],
    /// Where the next frame starts.
    at: usize,
}

impl<'a> JournalFrames<'a> {
    /// Starts every journal and states its version, so other bytes are refused at once.
    pub const TAG: &'static [u8] = b"campfire/journal/v1";

    /// The frames of `bytes`; an error when they do not start with the journal's tag.
    pub fn new(bytes: &'a [u8]) -> Result<JournalFrames<'a>, NotJournal> {
        if !bytes.starts_with(JournalFrames::TAG) {
            return Err(NotJournal);
        }
        Ok(JournalFrames {
            bytes,
            at: JournalFrames::TAG.len(),
        })
    }

    /// How many bytes the tag and the frames read so far take: once the read ends, the length
    /// to cut the journal to.
    pub const fn whole(&self) -> usize {
        self.at
    }

    /// Appends the frame of `record`, the bytes `out` holds from `start`, in place: its length
    /// before it, its hash after it.
    pub(crate) fn seal(out: &mut Vec<u8>, start: usize) {
        let len = out.len() - start - LEN_BYTES;
        assert!(len <= MAX_RECORD, "a journal record of {len} bytes");
        let len_bytes = u32::try_from(len).expect("a record within the bound fits u32");
        out[start..start + LEN_BYTES].copy_from_slice(&len_bytes.to_le_bytes());
        let hash = JournalFrames::hash(&out[start + LEN_BYTES..]);
        out.extend_from_slice(&hash);
    }

    /// Where a frame's record starts in `out`: its length's place is held first.
    pub(crate) fn open(out: &mut Vec<u8>) -> usize {
        let start = out.len();
        out.extend_from_slice(&[0; LEN_BYTES]);
        start
    }

    pub(crate) fn hash(record: &[u8]) -> [u8; HASH_BYTES] {
        let mut hasher = Hasher::new();
        hasher.update(FRAME_DOMAIN).update(record);
        *hasher.finalize().as_bytes()
    }
}

impl<'a> Iterator for JournalFrames<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<&'a [u8]> {
        let rest = &self.bytes[self.at..];
        let (len, rest) = rest.split_first_chunk::<LEN_BYTES>()?;
        let len = usize::try_from(u32::from_le_bytes(*len)).expect("u32 fits usize");
        if len > MAX_RECORD || rest.len() < len + HASH_BYTES {
            return None;
        }
        let (record, rest) = rest.split_at(len);
        if rest[..HASH_BYTES] != JournalFrames::hash(record) {
            return None;
        }
        self.at += LEN_BYTES + len + HASH_BYTES;
        Some(record)
    }
}
