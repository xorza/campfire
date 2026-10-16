use crate::zero_hour::error::W3dError;
use crate::zero_hour::w3d_chunks::W3dChunks;
use crate::zero_hour::w3d_fields::W3dFields;

/// A W3D HLOD: its name, its hierarchy's, its sub-objects, each a render object by its full
/// name on a pivot, and its aggregates, render objects the game adds on pivots whatever the
/// level of detail, as `HLodClass` reads them. Every shipped HLOD has one level of detail, the
/// one read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hlod {
    pub(crate) name: String,
    pub(crate) hierarchy: String,
    pub(crate) sub_objects: Vec<SubObject>,
    pub(crate) aggregates: Vec<SubObject>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubObject {
    pub(crate) pivot: usize,
    pub(crate) name: String,
}

const HEADER: u32 = 0x0701;
const LOD_ARRAY: u32 = 0x0702;
const SUB_OBJECT: u32 = 0x0704;
const AGGREGATE_ARRAY: u32 = 0x0705;

/// A name's bytes, and a sub-object's, `W3D_NAME_LEN` and twice it.
const NAME: usize = 16;
const LONG_NAME: usize = 2 * NAME;

impl Hlod {
    pub(crate) fn read(body: &[u8]) -> Result<Self, W3dError> {
        let mut header = None;
        let mut sub_objects = Vec::new();
        let mut aggregates = Vec::new();
        let mut arrays = 0;
        for chunk in W3dChunks::of(body) {
            let chunk = chunk?;
            match chunk.id {
                HEADER => {
                    let mut fields = W3dFields::of(chunk.body);
                    fields.u32()?;
                    let levels = fields.u32()?;
                    header = Some((levels, fields.name(NAME)?, fields.name(NAME)?));
                }
                LOD_ARRAY => {
                    arrays += 1;
                    Self::read_array(chunk.body, &mut sub_objects)?;
                }
                AGGREGATE_ARRAY => Self::read_array(chunk.body, &mut aggregates)?,
                _ => {}
            }
        }
        let (levels, name, hierarchy) = header.ok_or(W3dError::Missing)?;
        if levels != 1 {
            return Err(W3dError::Levels(levels));
        }
        if arrays != 1 {
            return Err(W3dError::LodArrays(arrays));
        }
        Ok(Self {
            name,
            hierarchy,
            sub_objects,
            aggregates,
        })
    }

    /// Each sub-object of the array `body`, into `into`.
    fn read_array(body: &[u8], into: &mut Vec<SubObject>) -> Result<(), W3dError> {
        for inner in W3dChunks::of(body) {
            let inner = inner?;
            if inner.id == SUB_OBJECT {
                let mut fields = W3dFields::of(inner.body);
                let pivot = usize::try_from(fields.u32()?).expect("a u32 fits usize");
                into.push(SubObject {
                    pivot,
                    name: fields.name(LONG_NAME)?,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use super::*;
    use crate::zero_hour::w3d_chunks::internals::chunk;
    use crate::zero_hour::w3d_fields::internals::name;

    impl Hlod {
        /// The HLOD chunk the game reads as this HLOD, of `levels` levels of detail and as many
        /// LOD arrays, each holding all its sub-objects, and an aggregate array when it has
        /// aggregates.
        pub(crate) fn chunk_of(&self, levels: u32) -> Vec<u8> {
            self.chunk_with(levels, levels)
        }

        /// The HLOD chunk of `levels` levels of detail in its header, and `arrays` LOD arrays.
        pub(crate) fn chunk_with(&self, levels: u32, arrays: u32) -> Vec<u8> {
            let header = [
                &[0; 4][..],
                &levels.to_le_bytes(),
                &name(&self.name, NAME),
                &name(&self.hierarchy, NAME),
            ]
            .concat();
            let array = |subs: &[SubObject]| {
                let count = u32::try_from(subs.len()).unwrap();
                let mut array = chunk(0x0703, false, &[count.to_le_bytes(), [0; 4]].concat());
                for sub in subs {
                    let pivot = u32::try_from(sub.pivot).unwrap();
                    array.extend(chunk(
                        SUB_OBJECT,
                        false,
                        &[&pivot.to_le_bytes()[..], &name(&sub.name, LONG_NAME)].concat(),
                    ));
                }
                array
            };
            let mut body = chunk(HEADER, false, &header);
            for _ in 0..arrays {
                body.extend(chunk(LOD_ARRAY, true, &array(&self.sub_objects)));
            }
            if !self.aggregates.is_empty() {
                body.extend(chunk(AGGREGATE_ARRAY, true, &array(&self.aggregates)));
            }
            chunk(0x0700, true, &body)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_hlod_reads_as_written_and_one_of_two_levels_is_refused() {
        let sub = |pivot, name: &str| SubObject {
            pivot,
            name: name.to_owned(),
        };
        let hlod = Hlod {
            name: "tank".to_owned(),
            hierarchy: "tank_skl".to_owned(),
            sub_objects: vec![
                sub(0, "tank.hull"),
                sub(1, "tank.a name of thirty-one bytes"),
            ],
            aggregates: vec![sub(1, "tank.smoke")],
        };
        assert_eq!(Hlod::read(&hlod.chunk_of(1)[8..]).unwrap(), hlod);
        assert_eq!(Hlod::read(&hlod.chunk_of(2)[8..]), Err(W3dError::Levels(2)));
        // One level of detail in its header and two arrays of it, which the game would read as
        // two levels.
        assert_eq!(
            Hlod::read(&hlod.chunk_with(1, 2)[8..]),
            Err(W3dError::LodArrays(2))
        );
        assert_eq!(
            Hlod::read(&hlod.chunk_with(1, 0)[8..]),
            Err(W3dError::LodArrays(0))
        );
        assert_eq!(Hlod::read(&[]), Err(W3dError::Missing));
    }
}
