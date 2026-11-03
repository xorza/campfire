use bevy_ecs::world::World;
use bevy_state::state::{NextState, State, States};
use campfire_console::SessionPhase;

/// Where the server's session is in its life, as Bevy's states hold it, so a system of one
/// phase names it in its run condition. A phase changes at the next state transition, in the
/// next frame; a session that ends within a frame stops its systems at once by its log's
/// result.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ServerPhase {
    /// No session yet: the app opens one, as `SimServerWorld::open_session` does.
    #[default]
    Opening,
    /// The lobby is open, and players join it.
    Lobby,
    /// A session a stop ended waits to restore in the next frame.
    Restoring,
    /// The match runs.
    Running,
    /// The match replays a published log to its viewers, as a local server opens a player's
    /// report (08-sessions.md, The client opens a report).
    Replaying,
}

impl ServerPhase {
    /// The phase of `world`'s server as of now: the one a change made in this frame goes to,
    /// before the next transition applies it.
    pub fn now(world: &World) -> Self {
        match world.resource::<NextState<Self>>() {
            NextState::Pending(next) | NextState::PendingIfDifferent(next) => *next,
            NextState::Unchanged => *world.resource::<State<Self>>().get(),
        }
    }

    /// Goes to it at the next transition.
    pub fn go(self, world: &mut World) {
        world.resource_mut::<NextState<Self>>().set(self);
    }
}

/// The console's phase of a session, the server's.
impl From<ServerPhase> for SessionPhase {
    fn from(phase: ServerPhase) -> Self {
        match phase {
            ServerPhase::Opening => Self::Opening,
            ServerPhase::Lobby => Self::Lobby,
            ServerPhase::Restoring => Self::Restoring,
            ServerPhase::Running => Self::Running,
            ServerPhase::Replaying => Self::Replaying,
        }
    }
}
