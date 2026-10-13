use crate::zero_hour::error::W3dError;
use crate::zero_hour::hierarchy::Hierarchy;
use crate::zero_hour::hlod::Hlod;
use crate::zero_hour::w3d_mesh::W3dMesh;

/// A W3D file, as `WW3DAssetManager` loads one (`w3d_file.h` in the released source): its meshes,
/// the names of its collision boxes and particle emitters, its hierarchies and its HLODs, the
/// chunks a model reads; others, as animations, are skipped.
#[derive(Debug, Default)]
pub(crate) struct W3dFile {
    pub(crate) meshes: Vec<W3dMesh>,
    pub(crate) boxes: Vec<String>,
    pub(crate) emitters: Vec<String>,
    pub(crate) hierarchies: Vec<Hierarchy>,
    pub(crate) hlods: Vec<Hlod>,
}

/// The chunks a file's top holds that a model reads.
const MESH: u32 = 0x0000;
const HIERARCHY: u32 = 0x0100;
const HLOD: u32 = 0x0700;
const BOX: u32 = 0x0740;
const EMITTER: u32 = 0x0500;
const EMITTER_HEADER: u32 = 0x0501;

/// A box's version and attributes, before its name of `2 * W3D_NAME_LEN` bytes.
const BOX_HEAD: usize = 8;
const BOX_NAME: usize = 32;
/// An emitter header's version, before its name of `W3D_NAME_LEN` bytes.
const EMITTER_HEAD: usize = 4;
const EMITTER_NAME: usize = 16;

/// A chunk's id and its body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Chunk<'a> {
    pub(crate) id: u32,
    pub(crate) body: &'a [u8],
}

/// The chunks of a body, one after another: each an id, a size whose top bit marks a body of
/// chunks, and the body.
#[derive(Debug, Clone)]
pub(crate) struct Chunks<'a> {
    rest: &'a [u8],
}

impl W3dFile {
    pub(crate) fn read(bytes: &[u8]) -> Result<W3dFile, W3dError> {
        let mut file = W3dFile::default();
        for chunk in Chunks::of(bytes) {
            let chunk = chunk?;
            match chunk.id {
                MESH => file.meshes.push(W3dMesh::read(chunk.body)?),
                HIERARCHY => file.hierarchies.push(Hierarchy::read(chunk.body)?),
                HLOD => file.hlods.push(Hlod::read(chunk.body)?),
                BOX => {
                    let mut fields = Fields::of(chunk.body);
                    fields.take(BOX_HEAD)?;
                    file.boxes.push(fields.name(BOX_NAME)?);
                }
                EMITTER => {
                    let header = Chunks::of(chunk.body)
                        .find(|inner| matches!(inner, Ok(inner) if inner.id == EMITTER_HEADER))
                        .ok_or(W3dError::Missing)??;
                    let mut fields = Fields::of(header.body);
                    fields.take(EMITTER_HEAD)?;
                    file.emitters.push(fields.name(EMITTER_NAME)?);
                }
                _ => {}
            }
        }
        Ok(file)
    }
}

impl<'a> Chunks<'a> {
    pub(crate) const fn of(bytes: &'a [u8]) -> Chunks<'a> {
        Chunks { rest: bytes }
    }
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>, W3dError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let Some((head, after)) = self.rest.split_first_chunk::<8>() else {
            self.rest = &[];
            return Some(Err(W3dError::Short));
        };
        let id = u32::from_le_bytes(head[..4].try_into().expect("4 bytes"));
        let size = u32::from_le_bytes(head[4..].try_into().expect("4 bytes")) & 0x7FFF_FFFF;
        let size = usize::try_from(size).expect("a u32 fits usize");
        let Some((body, rest)) = after.split_at_checked(size) else {
            self.rest = &[];
            return Some(Err(W3dError::Short));
        };
        self.rest = rest;
        Some(Ok(Chunk { id, body }))
    }
}

/// Little-endian fields of a chunk's body, each refused when it runs past the end.
#[derive(Debug, Clone)]
pub(crate) struct Fields<'a> {
    rest: &'a [u8],
}

impl<'a> Fields<'a> {
    pub(crate) const fn of(body: &'a [u8]) -> Fields<'a> {
        Fields { rest: body }
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], W3dError> {
        let (taken, rest) = self.rest.split_at_checked(len).ok_or(W3dError::Short)?;
        self.rest = rest;
        Ok(taken)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, W3dError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("2 bytes"),
        ))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, W3dError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    pub(crate) fn f32(&mut self) -> Result<f32, W3dError> {
        Ok(f32::from_le_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }

    pub(crate) fn vector(&mut self) -> Result<[f32; 3], W3dError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    /// A fixed field of `len` bytes holding a name up to its first NUL, in lowercase, as W3D
    /// compares names ignoring case.
    pub(crate) fn name(&mut self, len: usize) -> Result<String, W3dError> {
        let bytes = self.take(len)?;
        let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(len);
        let name = &bytes[..end];
        if !name.is_ascii() {
            return Err(W3dError::NotAscii);
        }
        Ok(String::from_utf8(name.to_ascii_lowercase()).expect("ASCII is UTF-8"))
    }

    /// What is left as `u32`s, which must be whole.
    pub(crate) fn u32s(&mut self) -> Result<Vec<u32>, W3dError> {
        Ok(self
            .records::<4>()?
            .into_iter()
            .map(u32::from_le_bytes)
            .collect())
    }

    /// What is left as whole records of `N` bytes.
    pub(crate) fn records<const N: usize>(&mut self) -> Result<Vec<[u8; N]>, W3dError> {
        let (records, rest) = self.take(self.rest.len())?.as_chunks::<N>();
        if rest.is_empty() {
            Ok(records.to_vec())
        } else {
            Err(W3dError::Short)
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use super::*;

    /// A chunk of `id` holding `body`, the top bit of its size set when `body` is chunks.
    pub(crate) fn chunk(id: u32, of_chunks: bool, body: &[u8]) -> Vec<u8> {
        let size = u32::try_from(body.len()).unwrap() | if of_chunks { 0x8000_0000 } else { 0 };
        [&id.to_le_bytes()[..], &size.to_le_bytes(), body].concat()
    }

    /// `name` in a field of `len` bytes, the rest NUL.
    pub(crate) fn name(name: &str, len: usize) -> Vec<u8> {
        assert!(name.len() <= len, "{name} fits {len} bytes");
        let mut field = name.as_bytes().to_vec();
        field.resize(len, 0);
        field
    }

    /// A collision box of the full name `full`, its color, center and extent zero.
    pub(crate) fn box_chunk(full: &str) -> Vec<u8> {
        let body = [&[0; BOX_HEAD][..], &name(full, BOX_NAME), &[0; 3 + 12 + 12]].concat();
        chunk(BOX, false, &body)
    }

    /// A particle emitter of `emitter`, its header alone.
    pub(crate) fn emitter_chunk(emitter: &str) -> Vec<u8> {
        let header = chunk(
            EMITTER_HEADER,
            false,
            &[&[0; EMITTER_HEAD][..], &name(emitter, EMITTER_NAME)].concat(),
        );
        chunk(EMITTER, true, &header)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::w3d_file::internals::{box_chunk, chunk, emitter_chunk, name};

    #[test]
    fn a_file_keeps_the_chunks_a_model_reads_and_refuses_one_past_its_end() {
        // An animation, 0x200, is skipped; the box and the emitter keep their names in lowercase.
        let bytes = [
            chunk(0x200, false, &[1, 2, 3]),
            box_chunk("TANK.PICKBOX"),
            emitter_chunk("Spray"),
        ]
        .concat();
        let file = W3dFile::read(&bytes).unwrap();
        assert_eq!(file.boxes, ["tank.pickbox"]);
        assert_eq!(file.emitters, ["spray"]);
        assert!(file.meshes.is_empty() && file.hierarchies.is_empty() && file.hlods.is_empty());
        // A size past the end, and a head of fewer than 8 bytes.
        let mut long = chunk(0x200, false, &[1, 2, 3]);
        long[4] = 4;
        assert!(matches!(W3dFile::read(&long), Err(W3dError::Short)));
        assert!(matches!(W3dFile::read(&[0; 7]), Err(W3dError::Short)));
        // An emitter with no header.
        assert!(matches!(
            W3dFile::read(&chunk(EMITTER, true, &[])),
            Err(W3dError::Missing)
        ));
    }

    #[test]
    fn fields_read_little_endian_and_names_up_to_their_nul() {
        let bytes = [
            &[0x34, 0x12][..],
            &0xDEAD_BEEF_u32.to_le_bytes(),
            &1.5_f32.to_le_bytes(),
            &name("MiXeD", 8),
            &[7, 0, 0, 0, 9, 0, 0, 0],
        ]
        .concat();
        let mut fields = Fields::of(&bytes);
        assert_eq!(fields.u16().unwrap(), 0x1234);
        assert_eq!(fields.u32().unwrap(), 0xDEAD_BEEF);
        assert_eq!(fields.f32().unwrap(), 1.5);
        assert_eq!(fields.name(8).unwrap(), "mixed");
        assert_eq!(fields.u32s().unwrap(), [7, 9]);
        assert!(matches!(fields.u32(), Err(W3dError::Short)));
        // A name of no NUL fills its field; one past ASCII is refused; a tail not whole is short.
        assert_eq!(Fields::of(b"ABCD").name(4).unwrap(), "abcd");
        assert!(matches!(
            Fields::of(&[0xE9, 0]).name(2),
            Err(W3dError::NotAscii)
        ));
        assert!(matches!(
            Fields::of(&[1, 2, 3]).records::<2>(),
            Err(W3dError::Short)
        ));
        assert!(matches!(
            Fields::of(&[1, 2, 3]).u32s(),
            Err(W3dError::Short)
        ));
    }
}
