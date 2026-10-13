use crate::zero_hour::error::W3dError;
use crate::zero_hour::w3d_file::{Chunks, Fields};

/// A W3D hierarchy: its name and its pivots, each placed in its parent's frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hierarchy {
    pub(crate) name: String,
    pub(crate) pivots: Vec<Pivot>,
}

/// A pivot, a bone: its name, its parent's index, before its own, and its translation and
/// rotation, a quaternion `x, y, z, w`, in that parent's frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pivot {
    pub(crate) name: String,
    pub(crate) parent: Option<usize>,
    pub(crate) translation: [f32; 3],
    pub(crate) rotation: [f32; 4],
}

const HEADER: u32 = 0x0101;
const PIVOTS: u32 = 0x0102;

/// A name's bytes, `W3D_NAME_LEN`.
const NAME: usize = 16;

/// A pivot's bytes: its name, its parent, its translation, its Euler angles, which the game
/// does not read, and its rotation.
const PIVOT: usize = NAME + 4 + 12 + 12 + 16;

impl Hierarchy {
    pub(crate) fn read(body: &[u8]) -> Result<Hierarchy, W3dError> {
        let mut name = None;
        let mut pivots = Vec::new();
        for chunk in Chunks::of(body) {
            let chunk = chunk?;
            let mut fields = Fields::of(chunk.body);
            match chunk.id {
                HEADER => {
                    fields.u32()?;
                    name = Some(fields.name(NAME)?);
                }
                PIVOTS => {
                    for (at, record) in fields.records::<PIVOT>()?.iter().enumerate() {
                        let mut pivot = Fields::of(record);
                        let name = pivot.name(NAME)?;
                        let parent = match pivot.u32()? {
                            u32::MAX => None,
                            parent => {
                                let parent = usize::try_from(parent).expect("a u32 fits usize");
                                // A parent comes before its child, so a walk to the root ends.
                                (parent < at).then_some(parent).ok_or(W3dError::Index)?;
                                Some(parent)
                            }
                        };
                        let translation = pivot.vector()?;
                        pivot.vector()?;
                        let rotation = [pivot.f32()?, pivot.f32()?, pivot.f32()?, pivot.f32()?];
                        if !translation
                            .iter()
                            .chain(&rotation)
                            .all(|float| float.is_finite())
                        {
                            return Err(W3dError::NotFinite);
                        }
                        pivots.push(Pivot {
                            name,
                            parent,
                            translation,
                            rotation,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(Hierarchy {
            name: name.ok_or(W3dError::Missing)?,
            pivots,
        })
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use super::*;
    use crate::zero_hour::w3d_file::internals::{chunk, name};

    impl Hierarchy {
        /// The hierarchy chunk the game reads as this hierarchy.
        pub(crate) fn chunk(&self) -> Vec<u8> {
            let count = u32::try_from(self.pivots.len()).unwrap();
            let header = [
                &4_u32.to_le_bytes()[..],
                &name(&self.name, NAME),
                &count.to_le_bytes(),
                &[0; 12],
            ]
            .concat();
            let pivots: Vec<u8> = self
                .pivots
                .iter()
                .flat_map(|pivot| {
                    let parent = pivot
                        .parent
                        .map_or(u32::MAX, |parent| u32::try_from(parent).unwrap());
                    let floats = |values: &[f32]| {
                        values
                            .iter()
                            .flat_map(|value| value.to_le_bytes())
                            .collect::<Vec<u8>>()
                    };
                    [
                        name(&pivot.name, NAME),
                        parent.to_le_bytes().to_vec(),
                        floats(&pivot.translation),
                        vec![0; 12],
                        floats(&pivot.rotation),
                    ]
                    .concat()
                })
                .collect();
            chunk(
                0x0100,
                true,
                &[chunk(HEADER, false, &header), chunk(PIVOTS, false, &pivots)].concat(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hierarchy_reads_as_written_and_a_parent_after_its_child_is_refused() {
        let pivot = |name: &str, parent| Pivot {
            name: name.to_owned(),
            parent,
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 1.0, 0.0],
        };
        let mut hierarchy = Hierarchy {
            name: "tank".to_owned(),
            pivots: vec![
                pivot("roottransform", None),
                pivot("turret", Some(0)),
                pivot("barrel", Some(1)),
            ],
        };
        assert_eq!(Hierarchy::read(&hierarchy.chunk()[8..]).unwrap(), hierarchy);
        hierarchy.pivots[1].parent = Some(1);
        assert_eq!(
            Hierarchy::read(&hierarchy.chunk()[8..]),
            Err(W3dError::Index)
        );
        hierarchy.pivots[1].parent = Some(2);
        assert_eq!(
            Hierarchy::read(&hierarchy.chunk()[8..]),
            Err(W3dError::Index)
        );
        hierarchy.pivots[1].parent = Some(0);
        hierarchy.pivots[2].rotation[3] = f32::NAN;
        assert_eq!(
            Hierarchy::read(&hierarchy.chunk()[8..]),
            Err(W3dError::NotFinite)
        );
    }
}
