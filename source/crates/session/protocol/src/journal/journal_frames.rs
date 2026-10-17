use campfire_common::{DomainHasher, HashDomain, Meaning, Value32};

use std::io::{self, Read};

use crate::journal::error::{JournalStreamError, NotJournal};

/// Starts the hash of every frame, so no other BLAKE3 use can produce one.
const FRAME_DOMAIN: HashDomain<FrameHash> = HashDomain::new("campfire/journal-frame/v1");
/// The longest record a frame holds: a longer length is a torn or corrupted frame.
pub(crate) const MAX_RECORD: usize = 16 << 20;
const LEN_BYTES: usize = size_of::<u32>();
const HASH_BYTES: usize = 32;

/// A frame's hash: the BLAKE3 of the frame domain and its record.
pub(crate) type FrameHash = Value32<FrameHashMeaning>;

/// The meaning of a `FrameHash`.
#[derive(Debug)]
pub(crate) enum FrameHashMeaning {}

impl Meaning for FrameHashMeaning {
    const NAME: &'static str = "FrameHash";
}

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
    pub const TAG: &'static [u8] = b"campfire/journal/v3";

    /// The frames of `bytes`; an error when they do not start with the journal's tag.
    pub fn new(bytes: &'a [u8]) -> Result<Self, NotJournal> {
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

    /// Reads the whole frames of the journal `reader` gives, in order, one at a time, and gives
    /// each record to `visit`, as the iterator reads them from bytes in memory: the read stops at
    /// the first frame that is short, too long, or whose hash fails. The memory it takes is one
    /// record's. An error for a reader that fails, and `NotJournal` for bytes that do not start
    /// with the journal's tag.
    pub fn stream(
        mut reader: impl Read,
        visit: &mut dyn FnMut(&[u8]),
    ) -> Result<(), JournalStreamError> {
        let mut tag = [0; Self::TAG.len()];
        if !read_whole(&mut reader, &mut tag)? || tag != *Self::TAG {
            return Err(NotJournal.into());
        }
        let mut record = Vec::new();
        loop {
            let mut len = [0; LEN_BYTES];
            if !read_whole(&mut reader, &mut len)? {
                return Ok(());
            }
            let len = usize::try_from(u32::from_le_bytes(len)).expect("u32 fits usize");
            if len > MAX_RECORD {
                return Ok(());
            }
            record.resize(len + HASH_BYTES, 0);
            if !read_whole(&mut reader, &mut record)? {
                return Ok(());
            }
            let (body, written) = record.split_at(len);
            if written != hash(body).as_bytes() {
                return Ok(());
            }
            visit(body);
        }
    }
}

/// Fills `buffer` from `reader`; whether it held that many bytes, none when it ends before.
fn read_whole(reader: &mut impl Read, buffer: &mut [u8]) -> Result<bool, JournalStreamError> {
    match reader.read_exact(buffer) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(JournalStreamError::Read(error)),
    }
}

/// The bytes a frame takes beside its record: its length and its hash.
pub(crate) const FRAME_OVERHEAD: usize = LEN_BYTES + HASH_BYTES;

/// Appends the frame of `record`, the bytes `out` holds from `start`, in place: its length
/// before it, its hash after it.
pub(crate) fn seal(out: &mut Vec<u8>, start: usize) {
    let len = out.len() - start - LEN_BYTES;
    debug_assert!(
        len <= MAX_RECORD,
        "a journal record of {len} bytes, past the bound its log's terms keep"
    );
    let len_bytes = u32::try_from(len).expect("a record within the bound fits u32");
    out[start..start + LEN_BYTES].copy_from_slice(&len_bytes.to_le_bytes());
    let hash = hash(&out[start + LEN_BYTES..]);
    out.extend_from_slice(hash.as_bytes());
}

/// Where a frame's record starts in `out`: its length's place is held first.
pub(crate) fn open(out: &mut Vec<u8>) -> usize {
    let start = out.len();
    out.extend_from_slice(&[0; LEN_BYTES]);
    start
}

/// The hash of the frame of `record`.
pub(crate) fn hash(record: &[u8]) -> FrameHash {
    let mut hasher = DomainHasher::new(FRAME_DOMAIN);
    hasher.update(record);
    hasher.finish()
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
        if rest[..HASH_BYTES] != *hash(record).as_bytes() {
            return None;
        }
        self.at += LEN_BYTES + len + HASH_BYTES;
        Some(record)
    }
}
