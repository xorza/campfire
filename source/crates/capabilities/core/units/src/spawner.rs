use bevy_ecs::bundle::{Bundle, NoBundleEffect};
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::MatchFacts;

use crate::spawn_at::SpawnAt;
use crate::spawn_hooks::SpawnHooks;
use crate::unit_entity::body_unit::BodyUnit;

/// How the match spawns a unit of a type that stands, owned by a player or none: its core with
/// the parts the caller gives, and the parts the modules declare, then the spawn hooks of the
/// match's capabilities, a type's passive among them. The mode, a capability that spawns a typed
/// unit, such as production, and a test call it. Only a match that computes its state spawns, as
/// a client receives its units from the server; the load checks that each spawn of the packages
/// names a type that stands, and a script's spawn is checked as its call is.
#[derive(Debug)]
pub struct Spawner;

impl Spawner {
    /// Spawns `at` in `world` with `parts`, facing its angle, and the parts the modules declare
    /// for a unit, but those of `parts`, in one move, then runs the capabilities' spawn hooks.
    pub fn spawn(
        world: &mut World,
        at: SpawnAt,
        parts: impl Bundle<Effect: NoBundleEffect>,
    ) -> Entity {
        debug_assert!(
            world.resource::<MatchFacts>().role.computes_state(),
            "only a match that computes its state spawns a unit"
        );
        Self::place(world, at, parts)
    }

    fn place(world: &mut World, at: SpawnAt, parts: impl Bundle<Effect: NoBundleEffect>) -> Entity {
        let entity = at.core().spawn(world, BodyUnit, parts);
        SpawnHooks::run(world, entity);
        entity
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::bundle::{Bundle, NoBundleEffect};
    use bevy_ecs::entity::Entity;
    use bevy_ecs::world::World;

    use crate::spawn_at::SpawnAt;
    use crate::spawner::Spawner;

    impl Spawner {
        /// Spawns as `spawn` does, on a client's match too, as a test sets one up with the units
        /// the server would send.
        pub fn place_any(
            world: &mut World,
            at: SpawnAt,
            parts: impl Bundle<Effect: NoBundleEffect>,
        ) -> Entity {
            Self::place(world, at, parts)
        }
    }
}
