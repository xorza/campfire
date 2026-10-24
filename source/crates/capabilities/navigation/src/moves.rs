use campfire_values::Team;
use serde::Deserialize;

/// Which units a walker may move out of its way, by a push or a step aside, as a mode's navigation
/// names them (04-capabilities/navigation.md, Who may move whom is the game's): its own team's, as
/// Zero Hour's units move their allies, or none, as Dota 2's units block one another.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Moves {
    #[default]
    Team,
    None,
}

impl Moves {
    /// Whether a unit of team `by` may move one of team `moved`: never a unit of no team.
    pub(crate) fn may_move(self, by: Option<Team>, moved: Option<Team>) -> bool {
        match self {
            Self::Team => by.is_some() && by == moved,
            Self::None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walker_moves_its_own_teams_units_or_none() {
        let [red, blue] = [0, 1].map(|team| Some(Team::new(team)));
        let cases = [(red, red), (red, blue), (None, None), (red, None)];
        let may = |moves: Moves| cases.map(|(by, moved)| moves.may_move(by, moved));
        assert_eq!(may(Moves::Team), [true, false, false, false]);
        assert_eq!(may(Moves::None), [false; 4]);
    }
}
