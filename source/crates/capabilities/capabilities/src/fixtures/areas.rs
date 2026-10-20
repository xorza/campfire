use bevy_ecs::world::World;
use campfire_areas::{Area, AreaSpec, Areas, Inside};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ByType;
use campfire_deliveries::Delivering;
use campfire_math::Num;
use campfire_script::Filter;
use campfire_sim::IdAllocator;
use campfire_values::{ActionId, Rank, RelationSet, UnitType};

use crate::fixtures::units::FilterFixtures;

/// The test fixtures of `Areas` that need a crate above its own.
pub trait AreasFixtures {
    /// An area unit of `unit_type`, a type that reaches `radius`, as a client draws it: the
    /// type's spec in the match's book of them, and the area, from tick 0 to tick 1.
    fn standing(world: &mut World, unit_type: UnitType, radius: Num) -> Area;
}

impl AreasFixtures for Areas {
    fn standing(world: &mut World, unit_type: UnitType, radius: Num) -> Area {
        let spec = AreaSpec {
            radius,
            delay: Ticks::new(0),
            duration: Ticks::new(1),
            affects: Filter::of_release_relations(RelationSet::All),
            inside: Inside::default(),
        };
        world
            .get_resource_or_init::<ByType<AreaSpec>>()
            .set(unit_type, spec);
        let by = Delivering {
            source: IdAllocator::default().allocate(),
            action: ActionId::new(0),
            rank: Rank::FIRST,
            start: None,
            launch: None,
        };
        Area::new(by, None, Tick::new(1))
    }
}
