use crate::mode::loadout_setup::LoadoutSetup;
use crate::mode::offer::Offer;
use crate::mode::unit_type_setup::UnitTypeSetup;
use crate::values::names::Names;

/// The mode's unit types that stand, its avatars' among them, its avatars by their packages'
/// names, in the order the mode depends on them, and its loadout's entries.
#[derive(Debug, Default)]
pub struct ModeUnits {
    pub(crate) unit_types: Vec<UnitTypeSetup>,
    pub(crate) avatars: Names<Offer>,
    pub(crate) loadout: LoadoutSetup,
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_sim::{IdAllocator, Position, StableId};

    use crate::mode::mode_units::ModeUnits;
    use crate::units::spawn_at::SpawnAt;
    use crate::units::team::Team;
    use crate::units::unit_type::UnitType;

    impl ModeUnits {
        /// A unit of the mode's type `unit_type` and `team` at `pos`, of the next stable id,
        /// with `parts`, spawned as the mode spawns one: with its type's kit, stats, slots and
        /// passive.
        pub fn spawn(
            &self,
            world: &mut World,
            unit_type: UnitType,
            pos: Position,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let setup = self
                .unit_types
                .iter()
                .find(|setup| setup.unit_type == unit_type)
                .expect("a unit type the mode spawns");
            let id = world.resource_mut::<IdAllocator>().allocate();
            let at = SpawnAt {
                id,
                unit_type,
                team,
                pos,
                angle: Num::ZERO,
            };
            setup
                .kit
                .spawn(world, at, None, &setup.actions, setup.passive, parts);
            id
        }
    }
}
