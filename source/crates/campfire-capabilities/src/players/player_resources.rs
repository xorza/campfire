use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::players::player::Player;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnChange;
use crate::units::owner::Owner;
use crate::units::view::View;

/// A player's resources, such as gold, on its entity: an amount for each resource the mode
/// declares, by its place in the mode's `resources`.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerResources(Vec<i64>);

impl PlayerResources {
    /// None of each of `resources` resources.
    pub(crate) fn new(resources: usize) -> PlayerResources {
        PlayerResources(vec![0; resources])
    }

    /// The resources of the player that owns a unit of `owner` in `world`: none for a unit no
    /// player owns, or in a match that keeps no players' resources.
    pub(crate) fn of<'w>(world: &'w World, owner: Option<&Owner>) -> Option<&'w PlayerResources> {
        PlayerResources::of_player(world, owner?.slot())
    }

    /// The resources of player `slot` in `world`: none for a player the session lacks, or in a
    /// match that keeps no players' resources.
    pub fn of_player(world: &World, slot: PlayerSlot) -> Option<&PlayerResources> {
        world.get(Player::entity(world, slot)?)
    }

    /// The resources of player `slot` in `world`, to change; none in a match that keeps no
    /// players' resources.
    pub(crate) fn of_mut(world: &mut World, slot: PlayerSlot) -> Option<Mut<'_, PlayerResources>> {
        let entity = Player::entity(world, slot)?;
        world.get_mut(entity)
    }

    /// How many resources each player of the match in `world` holds an amount of: those its
    /// mode declares, none before a mode.
    pub(crate) fn declared(world: &World) -> usize {
        world.non_send::<View>().resource_count()
    }

    /// Whether adding each of `amounts` keeps every amount within an `i64`.
    pub(crate) fn takes(&self, amounts: &[ResourceAmount]) -> bool {
        amounts
            .iter()
            .all(|add| self.amount(add.resource).checked_add(add.amount).is_some())
    }

    /// Adds each of `amounts`, unless one would carry its amount past an `i64`: then none, and
    /// `false`, so no part of a refund is lost.
    pub(crate) fn refund(&mut self, amounts: &[ResourceAmount]) -> bool {
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
    pub(crate) fn add(&mut self, resource: ResourceId, amount: i64) -> Option<()> {
        let at = self.at(resource);
        self.0[at] = self.0[at].checked_add(amount)?;
        Some(())
    }

    /// Takes `costs`, which it affords.
    pub(crate) fn pay(&mut self, costs: &[ResourceAmount]) {
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
        world.non_send::<View>().fits_resources(self.0.len())
    }
}

impl Replication for PlayerResources {
    const KIND: DataKind = DataKind::Resources;
    type Sending = SentOnChange;
}
