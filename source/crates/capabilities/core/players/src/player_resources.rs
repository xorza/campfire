use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::View;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnChange};
use campfire_units::Player;
use campfire_values::ResourceId;
use serde::{Deserialize, Serialize};

use crate::resource_amount::ResourceAmount;

/// A player's resources, such as gold, on its entity: an amount for each resource the mode
/// declares, by its place in the mode's `resources`.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerResources(Vec<i64>);

impl PlayerResources {
    /// None of each of `resources` resources.
    pub fn new(resources: usize) -> PlayerResources {
        PlayerResources(vec![0; resources])
    }

    /// Whether adding each of `amounts` keeps every amount within an `i64`.
    pub fn takes(&self, amounts: &[ResourceAmount]) -> bool {
        amounts
            .iter()
            .all(|add| self.amount(add.resource).checked_add(add.amount).is_some())
    }

    /// Adds each of `amounts`, unless one would carry its amount past an `i64`: then none, and
    /// `false`, so no part of a refund is lost.
    pub fn refund(&mut self, amounts: &[ResourceAmount]) -> bool {
        if !self.takes(amounts) {
            return false;
        }
        for refund in amounts {
            self.add(refund.resource, refund.amount)
                .expect("a refund the player's amounts take");
        }
        true
    }

    /// The amount of `resource` it holds.
    pub fn amount(&self, resource: ResourceId) -> i64 {
        self.0[self.at(resource)]
    }

    /// Adds `amount`; `None` when the sum overflows, which leaves it unchanged.
    pub fn add(&mut self, resource: ResourceId, amount: i64) -> Option<()> {
        let at = self.at(resource);
        self.0[at] = self.0[at].checked_add(amount)?;
        Some(())
    }

    /// Takes `costs`, which it affords.
    pub fn pay(&mut self, costs: &[ResourceAmount]) {
        for cost in costs {
            let at = self.at(cost.resource);
            debug_assert!(self.0[at] >= cost.amount, "the player affords the cost");
            self.0[at] -= cost.amount;
        }
    }

    fn at(&self, resource: ResourceId) -> usize {
        debug_assert!(
            resource.index() < self.0.len(),
            "a resource the mode declares"
        );
        resource.index()
    }
}

impl SimComponent for PlayerResources {
    const NAME: &'static str = "players.resources";
    type Host = Player;

    // A row of another width would place an amount under another resource.
    fn check(&self, world: &World, _: Entity) -> bool {
        world
            .non_send::<View>()
            .names()
            .fits_resources(self.0.len())
    }
}

impl Replication for PlayerResources {
    const KIND: DataKind = DataKind::Resources;
    type Sending = SentOnChange;
}
