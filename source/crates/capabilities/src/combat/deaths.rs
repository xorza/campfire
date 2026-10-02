use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_math::{PlayerSlot, Tick};
use campfire_sim::StableId;

use crate::units::owner::Owner;
use crate::units::team::Team;

/// The units that died in the tick `tick`, in the order they died, each with its team, its owner,
/// its killer and the units that assisted: what the mode receives in `on_unit_died`, and what the
/// server logs. Not state: it empties before each tick's damage, and the Mode stage of the same
/// tick reads it. Every unit it names exists until the end of its tick, as the dead despawn only
/// in the Vision stage, after the Mode stage; after that, only this record names them. A match
/// that ended runs no damage, so the record keeps its last tick's deaths.
#[derive(Resource, Debug, Default)]
pub struct Deaths {
    tick: Tick,
    entries: Vec<Death>,
    /// Each death's assisters, one run per death.
    assisters: Vec<StableId>,
}

#[derive(Debug, Clone)]
struct Death {
    fallen: Fallen,
    killer: Option<StableId>,
    assisters: Range<usize>,
}

/// A unit that died, as it was when it died: its team, and the player who owned it, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fallen {
    pub unit: StableId,
    pub team: Option<Team>,
    pub owner: Option<PlayerSlot>,
}

/// A death as `Deaths` gives it.
#[derive(Debug, Clone, Copy)]
pub struct DeathView<'a> {
    pub fallen: Fallen,
    pub killer: Option<StableId>,
    pub assisters: &'a [StableId],
}

impl Fallen {
    /// `unit` as it dies: of `team`, and owned by `owner`, if it has them.
    pub(crate) fn of(unit: StableId, team: Option<&Team>, owner: Option<&Owner>) -> Fallen {
        Fallen {
            unit,
            team: team.copied(),
            owner: owner.map(|owner| owner.slot()),
        }
    }
}

impl Deaths {
    /// Empties the record for the deaths of `tick`.
    pub(crate) fn clear(&mut self, tick: Tick) {
        self.tick = tick;
        self.entries.clear();
        self.assisters.clear();
    }

    /// Records that `fallen` died, dealt its last damage by `killer` if any, with `assisters`.
    pub(crate) fn push(
        &mut self,
        fallen: Fallen,
        killer: Option<StableId>,
        assisters: impl IntoIterator<Item = StableId>,
    ) {
        let start = self.assisters.len();
        self.assisters.extend(assisters);
        self.entries.push(Death {
            fallen,
            killer,
            assisters: start..self.assisters.len(),
        });
    }

    pub(crate) fn contains(&self, unit: StableId) -> bool {
        self.entries.iter().any(|death| death.fallen.unit == unit)
    }

    /// The tick whose deaths the record holds.
    pub const fn tick(&self) -> Tick {
        self.tick
    }

    /// The deaths, in the order the units died.
    pub fn iter(&self) -> impl Iterator<Item = DeathView<'_>> {
        self.entries.iter().map(|death| DeathView {
            fallen: death.fallen,
            killer: death.killer,
            assisters: &self.assisters[death.assisters.clone()],
        })
    }
}
