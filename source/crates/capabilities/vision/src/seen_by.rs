use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, NotSent, Replication};
use campfire_units::UnitEntity;
use campfire_values::TeamSet;
use serde::{Deserialize, Serialize};

/// The teams that see a unit, as the Vision stage of the last tick found them: those of its own
/// vision group, and of each group with a living unit that sees the grid cell it stands in. A
/// unit spawns with it on the server, before any Vision stage speaks of it, `SeenBy::UNTOLD`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeenBy(Option<TeamSet>);

impl SeenBy {
    /// Of a unit no Vision stage has spoken of: one that spawned after the last, or in a match
    /// whose map gives no grid. Each reader decides who sees it.
    pub(crate) const UNTOLD: SeenBy = SeenBy(None);

    pub const fn new(teams: TeamSet) -> SeenBy {
        SeenBy(Some(teams))
    }

    /// The teams that see the unit; `None` while no Vision stage has spoken of it.
    pub const fn get(self) -> Option<TeamSet> {
        self.0
    }
}

impl SimComponent for SeenBy {
    const NAME: &'static str = "vision.seen_by";
    type Host = UnitEntity;

    // A set of teams, which a reader only asks about the teams it has.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for SeenBy {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_values::TeamSet;

    use crate::seen_by::SeenBy;

    impl SeenBy {
        /// Seen by every team, for a test's unit in a match whose Vision stage never runs, as one
        /// with no map.
        pub const fn all() -> SeenBy {
            SeenBy::new(TeamSet::ALL)
        }
    }
}
