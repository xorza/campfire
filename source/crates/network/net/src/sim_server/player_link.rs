use bevy_ecs::component::Component;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::DeferredWorld;
use campfire_common::PlayerSlot;
use campfire_values::Team;

use crate::sim_server::viewer::Viewer;

/// Which player a client link carries the inputs of, their team, and whether the log refused one
/// of its messages, which ended the link. A link with one views the match as that player, and
/// only while it has one. Immutable: a change inserts a new one.
#[derive(Component, Debug, Clone, Copy)]
#[component(immutable, on_insert = view_as_player, on_remove = stop_viewing)]
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

    /// The link as the log refused one of its messages.
    pub(super) const fn into_refused(self) -> PlayerLink {
        PlayerLink {
            refused: true,
            ..self
        }
    }
}

fn view_as_player(mut world: DeferredWorld<'_>, context: HookContext) {
    let link = *world
        .get::<PlayerLink>(context.entity)
        .expect("on_insert runs on an entity with a PlayerLink");
    let viewer = Viewer::of_player(&world, link.slot(), link.team());
    world.commands().entity(context.entity).insert(viewer);
}

fn stop_viewing(mut world: DeferredWorld<'_>, context: HookContext) {
    world
        .commands()
        .entity(context.entity)
        .try_remove::<Viewer>();
}
