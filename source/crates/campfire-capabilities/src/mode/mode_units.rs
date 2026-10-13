use bevy_ecs::world::World;
use campfire_sim::StableId;

use crate::mode::loadout_setup::LoadoutSetup;
use crate::mode::offer::Offer;
use crate::mode::unit_type_setup::UnitTypeSetup;
use crate::navigation::Navigation;
use crate::stats::Stats;
use crate::stats::grants::Grant;
use crate::stats::lifetime::Hold;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::modifier_id::ModifierId;
use crate::units::spawner::Spawner;
use crate::units::unit_type::UnitType;
use crate::values::names::Names;
use crate::values::rank::Rank;

/// The mode's unit types that stand, its avatars' among them, its avatars by their packages'
/// names, in the order the mode depends on them, and its loadout's entries.
#[derive(Debug, Default)]
pub struct ModeUnits {
    pub(crate) unit_types: Vec<UnitTypeSetup>,
    pub(crate) avatars: Names<Offer>,
    pub(crate) loadout: LoadoutSetup,
}

impl ModeUnits {
    /// The spawner of the mode's unit types: a unit takes its type's passive, applied by itself,
    /// then its box body, if it has one, moves the walkers it lands on out of it.
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
            let boxed = unit
                .get::<Body>()
                .is_some_and(|body| body.half_edges().is_some());
            if let Some(&passive) = passives.get(*unit_type) {
                ModeUnits::apply_passive(world, id, passive);
            }
            if boxed {
                Navigation::make_room(world, entity);
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
