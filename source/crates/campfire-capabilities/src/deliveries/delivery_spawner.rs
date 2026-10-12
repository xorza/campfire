use std::fmt;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use bevy_ecs::world::EntityWorldMut;
use campfire_sim::{EntityIndex, IdAllocator, Position, StableId};

use crate::declare::entity_parts::EntityParts;
use crate::units::entity_filters::UnitFilter;
use crate::units::new_unit_states::NewUnitStates;
use crate::units::owner::Owner;
use crate::units::team::Team;
use crate::units::unit_core::UnitCore;
use crate::units::unit_entity::UnitEntity;
use crate::units::unit_state_book::UnitStateBook;
use crate::units::unit_type::UnitType;

/// Spawns the units that actions deliver, projectiles and areas, each for its source.
#[derive(SystemParam)]
pub(crate) struct DeliverySpawner<'w, 's> {
    commands: Commands<'w, 's>,
    ids: ResMut<'w, IdAllocator>,
    index: Res<'w, EntityIndex>,
    states: Res<'w, UnitStateBook>,
    new_states: ResMut<'w, NewUnitStates>,
    sources: Query<'w, 's, (&'static Team, Option<&'static Owner>), UnitFilter>,
}

/// `Commands` prints nothing, so a spawner prints only what it is.
impl fmt::Debug for DeliverySpawner<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeliverySpawner")
    }
}

impl DeliverySpawner<'_, '_> {
    /// Spawns a unit of `unit_type` at `pos` with what `make` gives for its stable id, `id`, which
    /// a script took for it, or else a new one, of `source`'s team and player, with its type's
    /// tags, the parts the modules declare for every unit, and its script state at their defaults
    /// but what the script wrote, and gives its id; none, and no id taken, when `source` is gone.
    pub(crate) fn spawn<B: Bundle>(
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
        let parts = core.made(self.states.initial(unit_type), &mut self.new_states);
        let mut unit = self.commands.spawn(make(id));
        parts.insert_into(&mut unit);
        unit.queue(|mut unit: EntityWorldMut<'_>| {
            EntityParts::<UnitEntity>::give(&mut unit);
        });
        Some(id)
    }
}
