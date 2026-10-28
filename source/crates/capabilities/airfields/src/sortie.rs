use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// Where a unit an airfield parks stands in its sortie (04-capabilities/airfields.md, A jet takes
/// off and lands on its runway; 04-capabilities/airfields.md, A jet goes home): in the air, idle
/// since a tick; at its place; waiting there for its runway; taxiing to its prep point, then to its
/// runway's start; waiting there until `until`; rolling down its runway and climbing out to its
/// approach, `step` meters a tick; flying home to its runway's approach; circling there for its
/// runway; sinking to its runway's end; rolling to its start; taxiing to its prep point, then to
/// its place; reloading there from `from` until `until`; or circling `at`, where it stood as it
/// went home to no airfield.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sortie {
    Flying { idle_since: Option<Tick> },
    Parked,
    Waiting,
    ToPrep,
    ToStart,
    Pause { until: Tick },
    Roll { step: Num },
    Home,
    Approach,
    Descent,
    Landing,
    BackToPrep,
    BackToPlace,
    Reload { from: Tick, until: Tick },
    Stranded { at: Vec3 },
}

impl Sortie {
    /// In the air, idle since no tick.
    pub const FLYING: Self = Self::Flying { idle_since: None };

    /// Whether its airfield holds it, from its runway's approach as it lands to its runway's end
    /// as it takes off: its walk and its attacks blocked, so an order waits for the air, as Zero
    /// Hour holds a command while a jet lands or takes off (`JetAIUpdate::aiDoCommand`).
    pub const fn held(self) -> bool {
        !self.flies()
    }

    /// Whether it flies by its own wings: in the air, on its way home, or stranded.
    pub const fn flies(self) -> bool {
        matches!(
            self,
            Self::Flying { .. } | Self::Home | Self::Stranded { .. }
        )
    }

    /// Whether it rests at its airfield off its runway, from its taxi off the runway as it lands
    /// to its roll as it takes off, where its airfield heals it, as Zero Hour heals a jet that
    /// may not fly (`JetAIUpdate::update`, `ParkingPlaceBehavior::setHealee`).
    pub const fn rests(self) -> bool {
        matches!(
            self,
            Self::BackToPrep
                | Self::BackToPlace
                | Self::Reload { .. }
                | Self::Parked
                | Self::Waiting
                | Self::ToPrep
                | Self::ToStart
                | Self::Pause { .. }
        )
    }
}

impl SimComponent for Sortie {
    const NAME: &'static str = "airfields.sortie";
    type Host = UnitEntity;

    // A roll of a step within the world's bound.
    fn check(&self, _: &World, _: Entity) -> bool {
        match *self {
            Self::Roll { step } => (Num::ZERO..=Vec3::BOUND).contains(&step),
            _ => true,
        }
    }
}

impl Replication for Sortie {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
