use bevy_ecs::resource::Resource;
use bevy_ecs::world::{Mut, World};
use campfire_script::{Ctx, Hook, ModeAnswer, ModeEvents, PendingCalls, PlayerHandle};
use campfire_sim::{PlayerSlotEvent, PlayerSlotEventKind, SimResource, TickInputs};
use serde::{Deserialize, Serialize};

use crate::match_mode::MatchMode;
use crate::mode_hooks::{OnPlayerJoin, OnPlayerLeave};

/// The joins and leaves whose hook has yet to run, in the order the log took them. The Mode stage
/// adds its tick's and runs the calls from the front; those whose call found the mode budget spent
/// stay, and run first in a later tick's Mode stage. So a tick ends with only those.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnansweredPlayerSlotEvents(pub PendingCalls<PlayerSlotEvent>);

impl SimResource for UnansweredPlayerSlotEvents {
    const NAME: &'static str = "mode.unanswered_player_slot_events";

    // An event of a player slot the session does not have reaches the mode's script as no player.
    fn check(&self, world: &World) -> bool {
        let players = world.resource::<MatchMode>().teams.players();
        self.0.iter().all(|event| event.player_slot.get() < players)
    }
}

/// A join runs `on_player_join`, and a leave `on_player_leave`, with the player's player slot, from
/// the mode's budget, in the order the log took them.
impl ModeEvents for UnansweredPlayerSlotEvents {
    const HOOKS: &'static [Hook] = &[Hook::of::<OnPlayerJoin>(), Hook::of::<OnPlayerLeave>()];

    fn arrive(world: &mut World) {
        if world
            .resource::<TickInputs>()
            .player_slot_events()
            .is_empty()
        {
            return;
        }
        world.resource_scope(|world, mut events: Mut<'_, Self>| {
            let new = world.resource::<TickInputs>().player_slot_events();
            events.0.extend(new.iter().copied());
        });
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn partial(&self) -> u8 {
        self.0.partial()
    }

    fn answer(&self, answer: &mut ModeAnswer<'_, '_>) -> usize {
        self.0
            .iter()
            .take_while(|event| {
                let args = |ctx: &Ctx| Some((ctx.clone(), PlayerHandle::new(event.player_slot)));
                let flow = match event.kind {
                    PlayerSlotEventKind::Joined => answer.call::<OnPlayerJoin>(None, args),
                    PlayerSlotEventKind::Left => answer.call::<OnPlayerLeave>(None, args),
                };
                flow.is_continue()
            })
            .count()
    }

    fn answered(&mut self, count: usize, partial: u8) {
        self.0.answered(count, partial);
    }

    fn append(&mut self, later: &mut Self) {
        self.0.append(&mut later.0);
    }

    fn hold(&self, _: usize, _: &mut World) {}
}
