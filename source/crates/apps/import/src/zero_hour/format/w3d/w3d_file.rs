use crate::zero_hour::format::w3d::error::W3dError;
use crate::zero_hour::format::w3d::hierarchy::Hierarchy;
use crate::zero_hour::format::w3d::hlod::Hlod;
use crate::zero_hour::format::w3d::w3d_chunks::W3dChunks;
use crate::zero_hour::format::w3d::w3d_fields::W3dFields;
use crate::zero_hour::format::w3d::w3d_mesh::W3dMesh;

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

impl W3dFile {
    /// Its hierarchy `name`; none for a name it holds none of.
    pub(crate) fn hierarchy(&self, name: &str) -> Option<&Hierarchy> {
        self.hierarchies
            .iter()
            .find(|hierarchy| hierarchy.name == name)
    }

    pub(crate) fn read(bytes: &[u8]) -> Result<Self, W3dError> {
        let mut file = Self::default();
        for chunk in W3dChunks::of(bytes) {
            let chunk = chunk?;
            match chunk.id {
                MESH => file.meshes.push(W3dMesh::read(chunk.body)?),
                HIERARCHY => file.hierarchies.push(Hierarchy::read(chunk.body)?),
                HLOD => file.hlods.push(Hlod::read(chunk.body)?),
                BOX => {
                    let mut fields = W3dFields::of(chunk.body);
                    fields.cursor.take(BOX_HEAD)?;
                    file.boxes.push(fields.name(BOX_NAME)?);
                }
                EMITTER => {
                    let header = W3dChunks::of(chunk.body).first_of(EMITTER_HEADER)?;
                    let mut fields = W3dFields::of(header.body);
                    fields.cursor.take(EMITTER_HEAD)?;
                    file.emitters.push(fields.name(EMITTER_NAME)?);
                }
                _ => {}
            }
        }
        Ok(file)
    }
}

#[cfg(test)]
pub(crate) mod samples {
    use super::*;
    use crate::zero_hour::format::w3d::w3d_chunks::samples::chunk;
    use crate::zero_hour::format::w3d::w3d_fields::samples::name;

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
    use crate::zero_hour::format::w3d::w3d_chunks::samples::chunk;
    use crate::zero_hour::format::w3d::w3d_file::samples::{box_chunk, emitter_chunk};

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
}
