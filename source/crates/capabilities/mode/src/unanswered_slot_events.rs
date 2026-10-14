use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_script::PendingCalls;
use campfire_sim::{SimResource, SlotEvent};
use serde::{Deserialize, Serialize};

use crate::match_mode::MatchMode;

/// The joins and leaves whose hook has yet to run, in the order the log took them. The Mode stage
/// adds its tick's and runs the calls from the front; those whose call found the mode pool spent
/// stay, and run first in a later tick's Mode stage. So a tick ends with only those.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnansweredSlotEvents(pub PendingCalls<SlotEvent>);

impl SimResource for UnansweredSlotEvents {
    const NAME: &'static str = "mode.unanswered_slot_events";

    // An event of a slot the session does not have reaches the mode's script as no player.
    fn check(&self, world: &World) -> bool {
        let players = world.non_send::<MatchMode>().teams.players();
        self.0.iter().all(|event| event.slot.get() < players)
    }
}
