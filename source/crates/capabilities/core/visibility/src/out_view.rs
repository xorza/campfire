use bevy_ecs::resource::Resource;
use campfire_common::{EnumAll, NamedEnum};

/// What a player out of play sees of the other teams (14-replication-audiences.md, A player out
/// of play sees what its team sees, or what a spectator sees). A match holds its session's as a
/// resource: a player that sees what a spectator sees gives no order. Each member's name is as
/// a host's setting writes it, and its place is its byte in a visibility's encoding, so a new
/// member goes last.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll, NamedEnum)]
pub enum OutView {
    /// What its team sees.
    Team,
    /// What a spectator sees.
    Spectators,
}
