use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::{ByType, PerType};
use campfire_sim::StableId;
use campfire_values::{ModifierId, Rank, UnitType};

use crate::Stats;
use crate::applier::Applier;
use crate::lifetime::Hold;
use crate::modifier_adds::ModifierAdd;
use crate::modifiers::Modifiers;

/// The modifier a unit type's units hold from their spawn on, from themselves: its passive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypePassive(pub ModifierId);

impl PerType for TypePassive {
    const SCRIPTS_ONLY: bool = false;
}

impl TypePassive {
    /// Grants the unit of `entity`, which the mode's spawner spawned, its type's passive, held
    /// from itself at the first rank, as `GrantPass` grants in its passes: the spawn hook of
    /// stats. A unit that spawns holds nothing from itself yet.
    pub(crate) fn grant(world: &mut World, entity: Entity) {
        let unit = world.entity(entity);
        let unit_type = *unit.get::<UnitType>().expect("a unit has a type");
        let id = *unit.get::<StableId>().expect("a unit has a stable id");
        let passive = world.resource::<ByType<Self>>().get(unit_type);
        let Some(&Self(modifier)) = passive else {
            return;
        };
        debug_assert!(
            unit.get::<Modifiers>()
                .is_some_and(|held| held.get(modifier, Some(id)).is_none()),
            "a unit that spawns holds its passive from no grant yet"
        );
        let applier = Applier {
            source: Some(id),
            action: None,
            rank: Rank::FIRST,
            hold: Some(Hold::Passive),
        };
        let add = ModifierAdd {
            target: id,
            id: modifier,
            applier,
            duration: None,
        };
        Stats::add_modifier(world, add);
    }
}
