use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::{ByType, PerType};
use campfire_sim::StableId;
use campfire_values::{ModifierId, Rank, UnitType};

use crate::Stats;
use crate::applier::Applier;
use crate::carried_mut::CarriedMut;
use crate::grants::{Grant, GrantChange};
use crate::lifetime::Hold;
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
    /// stats.
    pub(crate) fn grant(world: &mut World, entity: Entity) {
        let unit = world.entity(entity);
        let unit_type = *unit.get::<UnitType>().expect("a unit has a type");
        let id = *unit.get::<StableId>().expect("a unit has a stable id");
        let passive = world.resource::<ByType<Self>>().get(unit_type);
        let Some(&Self(modifier)) = passive else {
            return;
        };
        let grant = Grant {
            modifier,
            hold: Hold::Passive,
            action: None,
            rank: Some(Rank::FIRST),
        };
        let modifiers = world
            .get::<Modifiers>(entity)
            .expect("a unit holds its modifiers");
        match grant.change(modifiers, id) {
            Some(GrantChange::Apply(rank)) => {
                let applier = Applier {
                    source: Some(id),
                    action: grant.action,
                    rank,
                    hold: Some(grant.hold),
                };
                Stats::add_modifier(world, id, grant.modifier, applier, None);
            }
            Some(GrantChange::Release) => {
                if let Some(mut carried) = CarriedMut::of(world, entity) {
                    carried.release(grant.modifier, Some(id), grant.hold);
                }
            }
            None => {}
        }
    }
}
