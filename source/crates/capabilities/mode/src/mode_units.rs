use bevy_ecs::world::World;
use campfire_declare::ByType;
use campfire_sim::StableId;
use campfire_stats::{Grant, Hold, Stats};
use campfire_units::Spawner;
use campfire_values::{ModifierId, Names, Rank, UnitType};

use crate::loadout_setup::LoadoutSetup;
use crate::offer::Offer;
use crate::unit_type_setup::UnitTypeSetup;

/// The mode's unit types that stand, its avatars' among them, its avatars by their packages'
/// names, in the order the mode depends on them, and its loadout's entries.
#[derive(Debug, Default)]
pub struct ModeUnits {
    pub unit_types: Vec<UnitTypeSetup>,
    pub avatars: Names<Offer>,
    pub loadout: LoadoutSetup,
}

impl ModeUnits {
    /// The spawner of the mode's unit types: a unit takes its type's passive, applied by itself.
    pub(crate) fn spawner(&self) -> Spawner {
        let mut passives = ByType::default();
        for setup in &self.unit_types {
            passives.set_some(setup.unit_type, setup.passive);
        }
        let types = self.unit_types.iter().map(|setup| setup.unit_type);
        Spawner::new(types, move |world, entity| {
            let unit = world.entity(entity);
            let unit_type = unit.get::<UnitType>().expect("a unit has a type");
            let id = *unit.get::<StableId>().expect("a unit has a stable id");
            if let Some(&passive) = passives.get(*unit_type) {
                ModeUnits::apply_passive(world, id, passive);
            }
        })
    }

    /// Grants `passive` to the unit `id`, held from itself.
    fn apply_passive(world: &mut World, id: StableId, passive: ModifierId) {
        let grant = Grant {
            modifier: passive,
            hold: Hold::Passive,
            action: None,
            rank: Some(Rank::FIRST),
        };
        Stats::grant(world, id, grant);
    }
}
