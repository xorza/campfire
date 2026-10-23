use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::MatchNames;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_values::{NameId, ResourceId};
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::player::Player;
use crate::resource_amount::ResourceAmount;

/// A player's resources, such as gold, on its entity: an amount for each resource the mode
/// declares, by its place in the mode's `resources`.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerResources(Vec<i64>);

impl PlayerResources {
    /// None of each of `resources` resources.
    pub fn new(resources: usize) -> Self {
        Self(vec![0; resources])
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

    /// Adds `gain`, which is none or more, up to the most an amount holds and no further, so no
    /// gain is lost (04-capabilities/games.md, A gain stops at the most an amount holds).
    pub fn gain(&mut self, resource: ResourceId, gain: i64) {
        debug_assert!(gain >= 0, "a gain is none or more");
        let at = self.at(resource);
        self.0[at] = self.0[at].saturating_add(gain);
    }

    /// Adds `amount`; `None` when the sum overflows, which leaves it unchanged.
    pub fn add(&mut self, resource: ResourceId, amount: i64) -> Option<()> {
        let at = self.at(resource);
        self.0[at] = self.0[at].checked_add(amount)?;
        Some(())
    }

    /// Whether it holds each of `costs`.
    pub fn affords(&self, costs: &[ResourceAmount]) -> bool {
        costs
            .iter()
            .all(|cost| self.amount(cost.resource) >= cost.amount)
    }

    /// Takes `costs`, which it affords.
    pub fn pay(&mut self, costs: &[ResourceAmount]) {
        debug_assert!(self.affords(costs), "the player affords the cost");
        for cost in costs {
            let at = self.at(cost.resource);
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
        world.resource::<MatchNames>().fits_resources(self.0.len())
    }
}

impl Replication for PlayerResources {
    const KIND: DataKind = DataKind::Resources;
    type Sending = SentOnChange;
}

#[cfg(test)]
mod tests {
    use campfire_values::ResourceId;

    use crate::player_resources::PlayerResources;
    use crate::resource_amount::ResourceAmount;

    #[test]
    fn a_player_affords_a_cost_of_each_resource_it_holds_enough_of() {
        let [gold, gems] = [0, 1].map(ResourceId::new);
        let cost = |resource, amount| ResourceAmount { resource, amount };
        let mut held = PlayerResources::new(2);
        held.add(gold, 5).unwrap();
        // 5 gold and no gems: 5 gold is held, 6 is not, and a gem of the two is not either.
        assert!(held.affords(&[]));
        assert!(held.affords(&[cost(gold, 5)]));
        assert!(!held.affords(&[cost(gold, 6)]));
        assert!(!held.affords(&[cost(gold, 5), cost(gems, 1)]));
        held.pay(&[cost(gold, 5)]);
        assert_eq!([held.amount(gold), held.amount(gems)], [0, 0]);
        // A gain adds as an add does, and stops at the most an amount holds: 3 below it, a gain
        // of 5 leaves it at `i64::MAX`, where an add of 1 is refused and changes nothing.
        held.gain(gold, 7);
        assert_eq!(held.amount(gold), 7);
        held.add(gold, i64::MAX - 10).unwrap();
        held.gain(gold, 5);
        assert_eq!(held.amount(gold), i64::MAX);
        assert_eq!(held.add(gold, 1), None);
        assert_eq!(held.amount(gold), i64::MAX);
    }
}
