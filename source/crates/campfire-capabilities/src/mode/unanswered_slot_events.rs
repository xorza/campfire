use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::{SimResource, SlotEvent};
use serde::{Deserialize, Serialize};

use crate::mode::mode_book::ModeBook;
use crate::scripts::ctx::Ctx;

/// The joins and leaves whose hook has yet to run, in the order the log took them. The Mode stage
/// adds its tick's and runs the calls from the front; those whose call found the mode pool spent
/// stay, and run first in a later tick's Mode stage. So a tick ends with only those.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct UnansweredSlotEvents(pub(crate) Vec<SlotEvent>);

impl SimResource for UnansweredSlotEvents {
    const NAME: &'static str = "mode.unanswered_slot_events";

    // An event of a slot the session does not have reaches the mode's script as no player.
    fn check(&self, world: &World) -> bool {
        let book = world.get_non_send::<Ctx>().and_then(ModeBook::of);
        book.is_none_or(|book| {
            self.0
                .iter()
                .all(|event| event.slot.get() < book.teams.players())
        })
    }
}
