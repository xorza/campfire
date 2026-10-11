use std::num::NonZeroU32;

use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::NotSent;
use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{SimComponent, StableId};
use serde::{Deserialize, Serialize};

/// A node workers gather from: what it holds, and the worker that gathers it now, one at a time.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Node {
    amount: u32,
    holder: Option<StableId>,
}

impl Node {
    pub(crate) const fn new(amount: u32) -> Node {
        Node {
            amount,
            holder: None,
        }
    }

    pub(crate) const fn amount(self) -> u32 {
        self.amount
    }

    pub(crate) const fn holder(self) -> Option<StableId> {
        self.holder
    }

    pub(crate) const fn hold(&mut self, holder: Option<StableId>) {
        self.holder = holder;
    }

    /// Takes `take` from what it holds, or what is left: what was taken.
    pub(crate) fn take(&mut self, take: NonZeroU32) -> u32 {
        let taken = take.get().min(self.amount);
        self.amount -= taken;
        taken
    }
}

impl SimComponent for Node {
    const NAME: &'static str = "production.node";

    // An amount of any size, and a holder by any stable id, which the loop finds or not.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for Node {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
