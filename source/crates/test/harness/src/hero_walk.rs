use bevy_ecs::world::EntityRef;
use campfire_math::Vec3;
use campfire_sim::Position;
use campfire_units::Destination;

/// Where a hero stands and the place it walks to, as a test compares it across ticks and ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeroWalk {
    pub position: Vec3,
    pub destination: Destination,
}

impl HeroWalk {
    /// The walk of `hero`, a unit that stands and walks.
    pub fn of(hero: EntityRef<'_>) -> Self {
        Self {
            position: hero.get::<Position>().expect("a hero stands").get(),
            destination: *hero.get::<Destination>().expect("a hero walks"),
        }
    }
}
