use std::ops::Range;

use campfire_actions::{ActionBook, ShotLaunch};
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_math::{Num, SinCos, Vec3};
use campfire_script::UnknownName;
use campfire_units::Facing;
use campfire_values::{ActionId, Named, UnitType};

use crate::barrels::barrels_data::BarrelsData;

pub(crate) mod barrels_data;

/// A unit type's barrels as a match runs them (04-capabilities/combat.md, A shot leaves its
/// weapon's barrel): of each of its weapons that has them, by action in the order of their ids,
/// the run of its barrels, which its shots leave from in turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Barrels {
    weapons: Vec<(ActionId, Range<u32>)>,
    barrels: Vec<Barrel>,
}

/// A barrel: the point of its unit's frame where a shot's projectile starts, and the way of that
/// frame it leaves along, a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Barrel {
    pub(crate) at: Vec3,
    pub(crate) way: Vec3,
}

impl Barrels {
    /// The barrels of `action`, in the order its shots take them; none for a weapon of none.
    pub(crate) fn of(&self, action: ActionId) -> &[Barrel] {
        let at = self
            .weapons
            .binary_search_by_key(&action, |&(weapon, _)| weapon);
        at.map_or(&[], |at| {
            let Range { start, end } = self.weapons[at].1.clone();
            let place = |bound: u32| usize::try_from(bound).expect("a u32 fits a usize");
            &self.barrels[place(start)..place(end)]
        })
    }

    /// The barrels of each unit type that stands with some, each way made a unit; an error for a
    /// weapon its package does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let actions = cx.book::<ActionBook>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<BarrelsData>() else {
                continue;
            };
            let mut weapons = Vec::with_capacity(data.0.len());
            let mut barrels = Vec::new();
            for (name, of_weapon) in &data.0 {
                let action = actions.names().named((unit.package, name.as_str()));
                let action = action.ok_or_else(|| {
                    let at = BookPlace::UnitType(unit.place());
                    BookError::unknown(unit.package, at, UnknownName::new(ActionId::NAMES, name))
                })?;
                let start = u32::try_from(barrels.len()).expect("a type's barrels fit a u32");
                barrels.extend(of_weapon.iter().map(|barrel| {
                    Barrel {
                        at: barrel.at.get(),
                        way: barrel
                            .way
                            .normalized()
                            .expect("a barrel's way has a length"),
                    }
                }));
                let end = u32::try_from(barrels.len()).expect("a type's barrels fit a u32");
                weapons.push((action, start..end));
            }
            weapons.sort_unstable_by_key(|&(action, _)| action);
            book.set(unit.unit_type, Self { weapons, barrels });
        }
        Ok(book.build())
    }
}

impl PerId<UnitType> for Barrels {
    const SCRIPTS_ONLY: bool = false;
}

impl Barrel {
    /// It as the shot of a unit whose place stands at `point`, a point in space, facing `facing`,
    /// leaves it: turned by the facing about the unit's place, then moved there.
    pub(crate) fn launched(self, point: Vec3, facing: Option<Facing>) -> ShotLaunch {
        let turn = SinCos::of_degrees(facing.map_or(Num::ZERO, Facing::degrees));
        ShotLaunch {
            at: point + self.at.rotated_y(turn),
            way: self.way.rotated_y(turn),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Vec3;
    use campfire_values::ActionId;

    use crate::barrels::{Barrel, Barrels};

    impl Barrels {
        /// The barrels of `action` alone, each its point and its way, a unit, in the order its
        /// shots take them.
        pub fn of_weapon(action: ActionId, barrels: &[[Vec3; 2]]) -> Self {
            let end = u32::try_from(barrels.len()).expect("a weapon's barrels fit a u32");
            Self {
                weapons: vec![(action, 0..end)],
                barrels: barrels
                    .iter()
                    .map(|&[at, way]| Barrel { at, way })
                    .collect(),
            }
        }
    }
}
