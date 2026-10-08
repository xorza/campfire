use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

/// A timed life, as a ward's: the first tick the unit no longer lives, at the end of the tick
/// before which it despawns. State of the core.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Lifespan {
    ends: Tick,
}

impl Lifespan {
    pub(crate) const fn until(ends: Tick) -> Lifespan {
        Lifespan { ends }
    }

    pub const fn ends(self) -> Tick {
        self.ends
    }

    /// Whether its life ends with the tick that starts at `now`.
    pub(crate) fn ends_after(self, now: Tick) -> bool {
        self.ends <= now.after(Ticks::new(1))
    }
}

impl SimComponent for Lifespan {
    const NAME: &'static str = "units.lifespan";

    // Its decode keeps its tick within the limit.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}
