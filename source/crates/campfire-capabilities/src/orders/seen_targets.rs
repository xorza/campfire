use bevy_ecs::query::Allow;
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_sim::{EntityIndex, StableId, Unpredicted};

use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::vision::seen_by::SeenBy;
use crate::vision::vision_grid::VisionGrid;

/// The units a player's order may aim at: those its vision group sees as the order applies, as
/// the last Vision stage found them, so a stable id never tells a client what its group does not
/// see; every unit in a match with no vision grid. A player's units play on its team, so the
/// team of the unit it orders names its group. A client asks of the units it holds and does not
/// predict too, where the server last had them.
#[derive(SystemParam, Debug)]
pub(crate) struct SeenTargets<'w, 's> {
    index: Res<'w, EntityIndex>,
    grid: Option<Res<'w, VisionGrid>>,
    relations: Res<'w, Relations>,
    units: Query<'w, 's, (&'static Team, Option<&'static SeenBy>), Allow<Unpredicted>>,
}

impl SeenTargets<'_, '_> {
    /// Whether the vision group of `team` sees `target`; no group sees a unit that is gone. A
    /// unit that spawned after the last Vision stage, as one does before the match's first, is
    /// seen by its own group alone.
    pub(crate) fn sees(&self, team: Team, target: StableId) -> bool {
        let Some((&own, seen)) = self
            .index
            .get(target)
            .and_then(|entity| self.units.get(entity).ok())
        else {
            return false;
        };
        if self.grid.is_none() {
            return true;
        }
        match seen {
            Some(seen) => seen.get().contains(team),
            None => self.relations.vision_group(own).contains(team),
        }
    }
}
