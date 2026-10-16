use crate::zero_hour::format::byte_cursor::ByteCursor;
use crate::zero_hour::format::w3d::error::W3dError;

/// A chunk's id and its body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct W3dChunk<'a> {
    pub(crate) id: u32,
    pub(crate) body: &'a [u8],
}

/// The chunks of a body, one after another: each an id, a size whose top bit marks a body of
/// chunks, and the body.
#[derive(Debug, Clone)]
pub(crate) struct W3dChunks<'a> {
    rest: &'a [u8],
}

impl<'a> W3dChunks<'a> {
    pub(crate) const fn of(bytes: &'a [u8]) -> Self {
        W3dChunks { rest: bytes }
    }

    /// The first chunk of `id`; an error for a chunk before it that runs past its end, and for
    /// none.
    pub(crate) fn first_of(self, id: u32) -> Result<W3dChunk<'a>, W3dError> {
        for chunk in self {
            let chunk = chunk?;
            if chunk.id == id {
                return Ok(chunk);
            }
        }
        Err(W3dError::Missing)
    }
}

impl<'a> Iterator for W3dChunks<'a> {
    type Item = Result<W3dChunk<'a>, W3dError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let mut cursor = ByteCursor::new(self.rest);
        let mut read = || {
            let id = cursor.u32()?;
            let size = cursor.u32()? & 0x7FFF_FFFF;
            let body = cursor.take(usize::try_from(size).expect("a u32 fits usize"))?;
            Ok(W3dChunk { id, body })
        };
        let chunk = read();
        // A chunk cut short ends the walk.
        self.rest = if chunk.is_ok() { cursor.rest() } else { &[] };
        Some(chunk)
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// A chunk of `id` holding `body`, the top bit of its size set when `body` is chunks.
    pub(crate) fn chunk(id: u32, of_chunks: bool, body: &[u8]) -> Vec<u8> {
        let size = u32::try_from(body.len()).unwrap() | if of_chunks { 0x8000_0000 } else { 0 };
        [&id.to_le_bytes()[..], &size.to_le_bytes(), body].concat()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::w3d::w3d_chunks::internals::chunk;

    #[test]
    fn the_first_chunk_of_an_id_is_found_past_others_and_a_short_one_is_refused() {
        let bytes = [
            chunk(1, false, &[7]),
            chunk(2, false, &[8, 9]),
            chunk(2, false, &[]),
        ]
        .concat();
        let found = W3dChunks::of(&bytes).first_of(2).unwrap();
        assert_eq!((found.id, found.body), (2, &[8, 9][..]));
        assert!(matches!(
            W3dChunks::of(&bytes).first_of(3),
            Err(W3dError::Missing)
        ));
        // A chunk cut short before the one wanted is refused as short, not as missing.
        let cut = [chunk(1, false, &[7, 7]), chunk(2, false, &[])].concat();
        let short = &cut[..cut.len() - 9];
        assert!(matches!(
            W3dChunks::of(short).first_of(2),
            Err(W3dError::Short)
        ));
    }
}
