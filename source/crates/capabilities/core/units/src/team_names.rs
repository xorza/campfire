use campfire_declarations::Fact;
use campfire_values::{Names, Team};

/// The names of the match's playing teams, by team, in the manifest's order, which the mode
/// makes.
#[derive(Debug, Clone, Default)]
pub struct TeamNames(pub Names<Team>);

impl Fact for TeamNames {}
