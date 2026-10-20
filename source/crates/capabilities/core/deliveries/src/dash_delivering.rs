use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_state_types::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::{ActionStart, DashDelivery};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::delivering::Delivering;

/// The delivery its unit's dash is, of an instant action that started the dash in its
/// `on_resolve`, whose `on_end` runs as the dash ends: what it belongs to, and the meters the
/// dash went so far. It comes and goes with its unit's `ForcedMove`, in one bundle, so the move
/// holds only the movement.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashDelivering {
    pub by: Delivering,
    pub dashed: Num,
}

impl DashDelivering {
    /// The delivery `delivery`, before the dash went anywhere.
    pub fn new(delivery: DashDelivery) -> Self {
        let DashDelivery {
            source,
            action,
            rank,
            start,
        } = delivery;
        Self {
            by: Delivering {
                source: source.into(),
                action,
                rank,
                start,
                launch: None,
            },
            dashed: Num::ZERO,
        }
    }

    /// Adds the way of a step from `from` to `to`. A dash goes at most a step a tick, so only a
    /// match far past any session's length reaches the end of a number, where it stays.
    pub fn went(&mut self, from: Vec3, to: Vec3) {
        self.dashed = self
            .dashed
            .checked_add(from.distance(to))
            .unwrap_or(Num::MAX);
    }
}

impl SimComponent for DashDelivering {
    const NAME: &'static str = "deliveries.dash_delivering";
    type Host = UnitEntity;

    // Its own delivery, never a launch's; the start of its action lies within the world's bound,
    // as every place does, and the way it went is never negative.
    fn check(&self, _: &World, _: Entity) -> bool {
        let start = self.by.start.is_none_or(ActionStart::check);
        self.by.launch.is_none() && start && self.dashed >= Num::ZERO
    }
}

impl Replication for DashDelivering {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}
