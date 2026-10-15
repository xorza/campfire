use std::fmt;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, IdAllocator, Position, StableId};
use campfire_units::{DeliveryEntity, Owner, UnitCore, UnitFilter};
use campfire_values::{Team, UnitType};

/// Spawns the units that actions deliver, projectiles and areas, each for its source.
#[derive(SystemParam)]
pub struct DeliverySpawner<'w, 's> {
    commands: Commands<'w, 's>,
    ids: ResMut<'w, IdAllocator>,
    index: Res<'w, EntityIndex>,
    sources: Query<'w, 's, (&'static Team, Option<&'static Owner>), UnitFilter>,
}

/// `Commands` prints nothing, so a spawner prints only what it is.
impl fmt::Debug for DeliverySpawner<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeliverySpawner")
    }
}

impl DeliverySpawner<'_, '_> {
    /// Spawns a delivery of `unit_type` at `pos` with what `make` gives for its stable id, `id`,
    /// which a script took for it, or else a new one, of `source`'s team and player, through the
    /// one spawn of a unit, `UnitCore::spawn`, as the commands apply; gives its id; none, and
    /// no id taken, when `source` is gone.
    pub fn spawn<B: Bundle>(
        &mut self,
        source: StableId,
        pos: Position,
        unit_type: UnitType,
        id: Option<StableId>,
        make: impl FnOnce(StableId) -> B,
    ) -> Option<StableId> {
        let (&team, owner) = self
            .index
            .get(source)
            .and_then(|entity| self.sources.get(entity).ok())?;
        let id = id.unwrap_or_else(|| self.ids.allocate());
        let core = UnitCore {
            id,
            pos,
            unit_type,
            team,
            owner: owner.copied(),
        };
        let parts = make(id);
        self.commands.queue(move |world: &mut World| {
            core.spawn(world, DeliveryEntity, parts);
        });
        Some(id)
    }
}
