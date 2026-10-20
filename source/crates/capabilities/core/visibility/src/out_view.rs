use campfire_common::NamedEnum;

/// What a player out of play sees of the other teams (10-replication-audiences.md, A player out
/// of play sees what its team sees, or what a spectator sees). A match holds its session's in its
/// `MatchFacts`: a player that sees what a spectator sees gives no order. Each member's name is as
/// a host's setting writes it, and its place is its byte in a visibility's encoding, so a new
/// member goes last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, NamedEnum)]
pub enum OutView {
    /// What its team sees.
    Team,
    /// What a spectator sees.
    Spectators,
}
