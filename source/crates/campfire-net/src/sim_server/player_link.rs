use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use campfire_capabilities::Team;
use campfire_common::PlayerSlot;
use lightyear::prelude::Rooms;

use crate::sim_server::team_rooms::TeamRooms;

/// Which player a client link carries the inputs of, their team, and whether the log refused one
/// of its messages, which ended the link. A link with one stands in its team's room, and only
/// while it has one.
#[derive(Component, Debug, Clone, Copy)]
#[component(on_insert = enter_team_room, on_remove = leave_team_room)]
pub struct PlayerLink {
    slot: PlayerSlot,
    team: Team,
    refused: bool,
}

impl PlayerLink {
    pub(crate) const fn new(slot: PlayerSlot, team: Team) -> PlayerLink {
        PlayerLink {
            slot,
            team,
            refused: false,
        }
    }

    pub const fn slot(self) -> PlayerSlot {
        self.slot
    }

    /// Whether the log refused one of its messages: a broken chain or signature, or a limit
    /// passed. A client that follows the rules sends none, so the first one ends the link.
    pub const fn refused(self) -> bool {
        self.refused
    }

    pub(crate) const fn team(self) -> Team {
        self.team
    }

    pub(crate) const fn refuse(&mut self) {
        self.refused = true;
    }
}

fn enter_team_room(mut world: DeferredWorld<'_>, context: HookContext) {
    let team = world
        .get::<PlayerLink>(context.entity)
        .expect("on_insert runs on an entity with a PlayerLink")
        .team();
    let rooms = world.resource::<TeamRooms>().of(team);
    world.commands().entity(context.entity).insert(rooms);
}

fn leave_team_room(mut world: DeferredWorld<'_>, context: HookContext) {
    world
        .commands()
        .entity(context.entity)
        .try_remove::<Rooms>();
}
