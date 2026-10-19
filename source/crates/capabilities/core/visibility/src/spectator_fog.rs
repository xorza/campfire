use campfire_common::NamedEnum;

/// What a session's spectators see (10-replication-audiences.md, The host admits spectators,
/// and their fog is a setting), by the name a host's setting gives it. A member's place is its
/// byte in a visibility's encoding, so a new member goes last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, NamedEnum)]
pub enum SpectatorFog {
    /// Every unit, through any fog.
    All,
    /// One team's view, or all teams' together, as the spectator chooses.
    Teams,
}
