use crate::zero_hour::error::W3dError;
use crate::zero_hour::w3d_file::{Chunks, Fields};

/// A W3D HLOD: its name, its hierarchy's, and its sub-objects, each a render object by its full
/// name on a pivot. Every shipped HLOD has one level of detail, the one read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hlod {
    pub(crate) name: String,
    pub(crate) hierarchy: String,
    pub(crate) sub_objects: Vec<SubObject>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubObject {
    pub(crate) pivot: usize,
    pub(crate) name: String,
}

const HEADER: u32 = 0x0701;
const LOD_ARRAY: u32 = 0x0702;
const SUB_OBJECT: u32 = 0x0704;

/// A name's bytes, and a sub-object's, `W3D_NAME_LEN` and twice it.
const NAME: usize = 16;
const LONG_NAME: usize = 2 * NAME;

impl Hlod {
    pub(crate) fn read(body: &[u8]) -> Result<Hlod, W3dError> {
        let mut header = None;
        let mut sub_objects = Vec::new();
        let mut arrays = 0;
        for chunk in Chunks::of(body) {
            let chunk = chunk?;
            match chunk.id {
                HEADER => {
                    let mut fields = Fields::of(chunk.body);
                    fields.u32()?;
                    let levels = fields.u32()?;
                    header = Some((levels, fields.name(NAME)?, fields.name(NAME)?));
                }
                LOD_ARRAY => {
                    arrays += 1;
                    for inner in Chunks::of(chunk.body) {
                        let inner = inner?;
                        if inner.id == SUB_OBJECT {
                            let mut fields = Fields::of(inner.body);
                            let pivot = usize::try_from(fields.u32()?).expect("a u32 fits usize");
                            sub_objects.push(SubObject {
                                pivot,
                                name: fields.name(LONG_NAME)?,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        let (levels, name, hierarchy) = header.ok_or(W3dError::Missing)?;
        if levels != 1 || arrays != 1 {
            return Err(W3dError::Levels(levels));
        }
        Ok(Hlod {
            name,
            hierarchy,
            sub_objects,
        })
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use super::*;
    use crate::zero_hour::w3d_file::internals::{chunk, name};

    impl Hlod {
        /// The HLOD chunk the game reads as this HLOD, of `levels` levels of detail, each
        /// holding all its sub-objects.
        pub(crate) fn chunk_of(&self, levels: u32) -> Vec<u8> {
            let header = [
                &[0; 4][..],
                &levels.to_le_bytes(),
                &name(&self.name, NAME),
                &name(&self.hierarchy, NAME),
            ]
            .concat();
            let count = u32::try_from(self.sub_objects.len()).unwrap();
            let mut array = chunk(0x0703, false, &[count.to_le_bytes(), [0; 4]].concat());
            for sub in &self.sub_objects {
                let pivot = u32::try_from(sub.pivot).unwrap();
                array.extend(chunk(
                    SUB_OBJECT,
                    false,
                    &[&pivot.to_le_bytes()[..], &name(&sub.name, LONG_NAME)].concat(),
                ));
            }
            let mut body = chunk(HEADER, false, &header);
            for _ in 0..levels {
                body.extend(chunk(LOD_ARRAY, true, &array));
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
        };
        assert_eq!(Hlod::read(&hlod.chunk_of(1)[8..]).unwrap(), hlod);
        assert_eq!(Hlod::read(&hlod.chunk_of(2)[8..]), Err(W3dError::Levels(2)));
        assert_eq!(Hlod::read(&[]), Err(W3dError::Missing));
    }
}
