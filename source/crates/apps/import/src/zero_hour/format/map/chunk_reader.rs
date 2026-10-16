use std::ops::RangeInclusive;

use crate::zero_hour::format::byte_cursor::ByteCursor;
use crate::zero_hour::format::map::error::{Chunk, MapReadError};

/// A map's bytes and how far a read has taken them, as the game's `DataChunkInput` reads them:
/// its cursor's fields, and a map's counts, strings and chunks.
#[derive(Debug, Clone)]
pub(crate) struct ChunkReader<'a> {
    pub(crate) cursor: ByteCursor<'a>,
}

/// A chunk's header and its body: its name's id in the file's table, and its version.
#[derive(Debug)]
pub(crate) struct RawChunk<'a> {
    pub(crate) id: u32,
    pub(crate) version: u16,
    pub(crate) body: ChunkReader<'a>,
}

impl<'a> ChunkReader<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        ChunkReader {
            cursor: ByteCursor::new(bytes),
        }
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.cursor.is_empty()
    }

    /// Refuses a body of the chunk of `kind` with bytes left past its fields.
    pub(crate) const fn ended(&self, kind: Chunk) -> Result<(), MapReadError> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(MapReadError::Leftover(kind))
        }
    }

    /// An `i32` the game keeps unsigned, refused when negative.
    pub(crate) fn non_negative(&mut self) -> Result<u32, MapReadError> {
        let value = self.cursor.i32()?;
        u32::try_from(value)
            .ok()
            .ok_or(MapReadError::Negative(value))
    }

    /// An `i32` count, refused when negative.
    pub(crate) fn count(&mut self) -> Result<usize, MapReadError> {
        let count = self.cursor.i32()?;
        usize::try_from(count)
            .ok()
            .ok_or(MapReadError::Negative(count))
    }

    /// `count` little-endian `i16`s.
    pub(crate) fn i16s(&mut self, count: usize) -> Result<Vec<i16>, MapReadError> {
        let pairs = self.cursor.records::<2>(count)?;
        Ok(pairs.iter().map(|pair| i16::from_le_bytes(*pair)).collect())
    }

    /// A string as the game writes one: a `u16` length and its bytes.
    pub(crate) fn text(&mut self) -> Result<&'a [u8], MapReadError> {
        let len = self.cursor.u16()?;
        Ok(self.cursor.take(len.into())?)
    }

    /// The next chunk: its id, its version, and an `i32` size of body, which must lie within
    /// these bytes.
    pub(crate) fn chunk(&mut self) -> Result<RawChunk<'a>, MapReadError> {
        let id = self.cursor.u32()?;
        let version = self.cursor.u16()?;
        let size = self.count()?;
        Ok(RawChunk {
            id,
            version,
            body: ChunkReader::new(self.cursor.take(size)?),
        })
    }
}

impl RawChunk<'_> {
    /// Refuses a chunk of `kind` of a version not among `versions`.
    pub(crate) fn check_version(
        &self,
        kind: Chunk,
        versions: RangeInclusive<u16>,
    ) -> Result<(), MapReadError> {
        if versions.contains(&self.version) {
            Ok(())
        } else {
            Err(MapReadError::Version {
                chunk: kind,
                version: self.version,
            })
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    /// A map file built by hand, as the game's `DataChunkOutput` writes one: each name it uses
    /// gets an id from 1 in its table.
    #[derive(Debug, Default)]
    pub(crate) struct ChunkWriter {
        names: Vec<String>,
    }

    impl ChunkWriter {
        pub(crate) fn id(&mut self, name: &str) -> u32 {
            let at = if let Some(at) = self.names.iter().position(|known| known == name) {
                at
            } else {
                self.names.push(name.to_owned());
                self.names.len() - 1
            };
            u32::try_from(at + 1).unwrap()
        }

        /// Each name used, by its id, as a reader's table holds them.
        pub(crate) fn table(&self) -> BTreeMap<u32, &[u8]> {
            (1..)
                .zip(&self.names)
                .map(|(id, name)| (id, name.as_bytes()))
                .collect()
        }

        /// A chunk of `name` and `version` around `body`.
        pub(crate) fn chunk(&mut self, name: &str, version: u16, body: &[u8]) -> Vec<u8> {
            let size = i32::try_from(body.len()).unwrap();
            [
                &self.id(name).to_le_bytes()[..],
                &version.to_le_bytes(),
                &size.to_le_bytes(),
                body,
            ]
            .concat()
        }

        /// A dictionary entry: its key's id and its type in one `i32`, then its value.
        pub(crate) fn entry(&mut self, key: &str, kind: u8, value: &[u8]) -> Vec<u8> {
            let key_and_type = (self.id(key) << 8) | u32::from(kind);
            [&key_and_type.to_le_bytes()[..], value].concat()
        }

        /// A string's `u16` length and bytes.
        pub(crate) fn text(bytes: &[u8]) -> Vec<u8> {
            [
                &u16::try_from(bytes.len()).unwrap().to_le_bytes()[..],
                bytes,
            ]
            .concat()
        }

        /// The file: `CkMp`, the table of every name used, and `chunks`.
        pub(crate) fn file(&self, chunks: &[Vec<u8>]) -> Vec<u8> {
            let mut file = b"CkMp".to_vec();
            file.extend(i32::try_from(self.names.len()).unwrap().to_le_bytes());
            for (at, name) in self.names.iter().enumerate() {
                file.push(u8::try_from(name.len()).unwrap());
                file.extend(name.as_bytes());
                file.extend(u32::try_from(at + 1).unwrap().to_le_bytes());
            }
            file.extend(chunks.concat());
            file
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::error::Short;

    #[test]
    fn fields_read_little_endian_and_stop_at_the_end() {
        let bytes = [
            &[7][..],
            &[0x34, 0x12],
            &(-2_i32).to_le_bytes(),
            &1.5_f32.to_le_bytes(),
            &[2, 0, b'h', b'i'],
            &[0xFE, 0xFF, 3, 0],
            // A chunk of id 9, version 4 and 1 byte, then a byte past it.
            &[9, 0, 0, 0, 4, 0, 1, 0, 0, 0, 0xAA, 0xBB],
        ]
        .concat();
        let mut reader = ChunkReader::new(&bytes);
        assert_eq!(reader.cursor.u8(), Ok(7));
        assert_eq!(reader.cursor.u16(), Ok(0x1234));
        assert_eq!(reader.clone().count(), Err(MapReadError::Negative(-2)));
        assert_eq!(reader.cursor.i32(), Ok(-2));
        assert_eq!(reader.cursor.f32(), Ok(1.5));
        assert_eq!(reader.text(), Ok(&b"hi"[..]));
        assert_eq!(reader.i16s(2), Ok(vec![-2, 3]));
        let mut chunk = reader.chunk().unwrap();
        assert_eq!((chunk.id, chunk.version), (9, 4));
        assert_eq!(chunk.body.cursor.u8(), Ok(0xAA));
        assert!(chunk.body.is_empty());
        assert_eq!(reader.cursor.u16(), Err(Short));
        assert_eq!(reader.cursor.u8(), Ok(0xBB));
        assert!(reader.is_empty());
        // A chunk whose size runs past the bytes.
        let long = [9, 0, 0, 0, 4, 0, 2, 0, 0, 0, 0xAA];
        assert_eq!(
            ChunkReader::new(&long).chunk().map(|_| ()),
            Err(MapReadError::Short)
        );
    }
}
