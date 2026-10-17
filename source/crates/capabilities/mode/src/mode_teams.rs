use crate::team_manifest::TeamManifest;

/// The playing teams of the mode's manifest, in its order: what the load gives the books.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeTeams(pub Vec<TeamManifest>);
