use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// Where a unit an airfield parks stands in its sortie (jets.md, A jet takes off and lands on its
/// runway; jets.md, A jet goes home): in the air, idle since a tick; at its place; waiting there
/// for its runway; taxiing to its prep point, then to its runway's start; waiting there until
/// `until`; rolling down its runway; flying home to its runway's approach; circling there for its
/// runway; sinking to its runway's end; rolling to its start; taxiing to its prep point, then to
/// its place; or reloading there from `from` until `until`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sortie {
    Flying { idle_since: Option<Tick> },
    Parked,
    Waiting,
    ToPrep,
    ToStart,
    Pause { until: Tick },
    Roll,
    Home,
    Approach,
    Descent,
    Landing,
    BackToPrep,
    BackToPlace,
    Reload { from: Tick, until: Tick },
}

impl Sortie {
    /// In the air, idle since no tick.
    pub const FLYING: Self = Self::Flying { idle_since: None };

    /// Whether its airfield holds it on the ground, from its runway's end as it lands to its
    /// runway's end as it takes off: its walk and its attacks blocked.
    pub const fn grounded(self) -> bool {
        !matches!(self, Self::Flying { .. } | Self::Home | Self::Approach)
    }

    /// Whether it flies by its own wings: in the air, on its way home or at its approach.
    pub const fn flies(self) -> bool {
        matches!(self, Self::Flying { .. } | Self::Home | Self::Approach)
    }
}

impl SimComponent for Sortie {
    const NAME: &'static str = "airfields.sortie";
    type Host = UnitEntity;

    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Sortie {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
