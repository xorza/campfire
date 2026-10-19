use std::fmt;

use bevy_ecs::bundle::{Bundle, NoBundleEffect};
use bevy_ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{EntityIndex, IdAllocator, StableId};
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

/// A delivery to spawn: its source, its type, where it lands, and the stable id a script took
/// for it, if one did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewDelivery {
    pub source: StableId,
    pub unit_type: UnitType,
    pub position: Vec3,
    pub id: Option<StableId>,
}

impl DeliverySpawner<'_, '_> {
    /// Spawns `delivery` with what `make` gives for its stable id, the one its script took, or
    /// else a new one, of its source's team and player, through the one spawn of a unit,
    /// `UnitCore::spawn`, as the commands apply; gives its id; none, and no id taken, when its
    /// source is gone.
    pub fn spawn<B: Bundle<Effect: NoBundleEffect>>(
        &mut self,
        delivery: NewDelivery,
        make: impl FnOnce(StableId) -> B,
    ) -> Option<StableId> {
        let NewDelivery {
            source,
            unit_type,
            position,
            id,
        } = delivery;
        let (&team, owner) = self
            .index
            .get(source)
            .and_then(|entity| self.sources.get(entity).ok())?;
        let id = id.unwrap_or_else(|| self.ids.allocate());
        let core = UnitCore {
            id,
            position,
            unit_type,
            team,
            owner: owner.copied(),
            angle: Num::ZERO,
        };
        let parts = make(id);
        self.commands.queue(move |world: &mut World| {
            core.spawn(world, DeliveryEntity, parts);
        });
        Some(id)
    }
}
