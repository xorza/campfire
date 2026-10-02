use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_math::PlayerSlot;
use campfire_sim::SimResource;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::players::resource_id::{ResourceAmount, ResourceId};
use crate::units::script_view::View;

/// The players' resources, such as gold: one run of amounts by player slot, then by the
/// resource's place in the mode's `resources`.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerResources {
    /// How many resources the mode declares: the amounts of one player.
    resources: usize,
    amounts: Vec<i64>,
}

impl PlayerResources {
    /// None of `resources` resources for each of `players` players.
    pub(crate) fn new(players: usize, resources: usize) -> PlayerResources {
        PlayerResources {
            resources,
            amounts: vec![0; players * resources],
        }
    }

    /// The amount of `resource` player `slot` holds.
    pub fn amount(&self, slot: PlayerSlot, resource: ResourceId) -> i64 {
        self.amounts[self.at(slot, resource)]
    }

    /// Adds `amount`; `None` when the sum overflows, which leaves it unchanged.
    pub(crate) fn add(
        &mut self,
        slot: PlayerSlot,
        resource: ResourceId,
        amount: i64,
    ) -> Option<()> {
        let at = self.at(slot, resource);
        self.amounts[at] = self.amounts[at].checked_add(amount)?;
        Some(())
    }

    /// Takes `costs` from player `slot`, who affords them.
    pub(crate) fn pay(&mut self, slot: PlayerSlot, costs: &[ResourceAmount]) {
        for cost in costs {
            let at = self.at(slot, cost.resource);
            debug_assert!(
                self.amounts[at] >= cost.amount,
                "the player affords the cost"
            );
            self.amounts[at] -= cost.amount;
        }
    }

    fn at(&self, slot: PlayerSlot, resource: ResourceId) -> usize {
        debug_assert!(
            resource.index() < self.resources,
            "a resource the mode declares"
        );
        slot.index() * self.resources + resource.index()
    }
}

/// A snapshot is untrusted, so amounts that make no whole row of each player fail to decode.
impl<'de> Deserialize<'de> for PlayerResources {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<PlayerResources, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            resources: usize,
            amounts: Vec<i64>,
        }
        let Fields { resources, amounts } = Fields::deserialize(deserializer)?;
        if amounts
            .len()
            .checked_rem(resources)
            .is_some_and(|rest| rest != 0)
        {
            return Err(D::Error::custom("amounts that make no whole rows"));
        }
        Ok(PlayerResources { resources, amounts })
    }
}

impl SimResource for PlayerResources {
    const NAME: &'static str = "mode.player_resources";

    // A row of another width, or a row of a player the session lacks, would place an amount
    // under another player or resource.
    fn check(&self, world: &World) -> bool {
        world
            .get_non_send::<View>()
            .is_none_or(|view| view.fits_resources(self.resources, self.amounts.len()))
    }
}
