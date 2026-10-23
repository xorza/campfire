use std::num::NonZeroU32;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::PerId;
use campfire_sim::{SimComponent, StableId};
use campfire_states::{Replication, SentPredicted};
use campfire_units::UnitEntity;
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// A node workers gather from: what it holds, and the worker that gathers it now, one at a time.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    amount: u32,
    holder: Option<StableId>,
}

impl Node {
    pub const fn new(amount: u32) -> Self {
        Self {
            amount,
            holder: None,
        }
    }

    pub const fn amount(self) -> u32 {
        self.amount
    }

    pub const fn holder(self) -> Option<StableId> {
        self.holder
    }

    pub(crate) const fn hold(&mut self, holder: Option<StableId>) {
        self.holder = holder;
    }

    /// What a take of `take` takes: `take`, or what is left.
    pub(crate) fn taken(self, take: NonZeroU32) -> u32 {
        take.get().min(self.amount)
    }

    /// Takes `take` from what it holds, or what is left: what was taken.
    pub fn take(&mut self, take: NonZeroU32) -> u32 {
        let taken = self.taken(take);
        self.amount -= taken;
        taken
    }
}

impl SimComponent for Node {
    const NAME: &'static str = "production.node";
    type Host = UnitEntity;

    // An amount of any size, and a holder by any stable id, which the loop finds or not.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Node {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentPredicted;
}

impl PerId<UnitType> for Node {
    const SCRIPTS_ONLY: bool = false;
}
