use campfire_common::DeclaredName;
use campfire_values::TextId;

/// The player slots of a game, `MAX_PLAYER_SLOTS`: the starts a map may name, from
/// `Player_1_Start`, and the players of a skirmish, each its own team, as the original's default
/// skirmish holds them (09-zero-hour.md, Eight teams of one).
pub(crate) const SLOTS: u8 = 8;

/// A team of the imported mode: a player's own, of a slot from 1 to `SLOTS`, or the neutral team
/// every placed object stands on until the map's sides are read (09-zero-hour.md, Eight teams of
/// one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportTeam {
    Player(u8),
    Neutral,
}

impl ImportTeam {
    /// Every team, the players' in their slots' order, then the neutral one.
    pub(crate) fn all() -> impl Iterator<Item = Self> {
        Self::players().chain([Self::Neutral])
    }

    /// The players' teams, in their slots' order.
    pub(crate) fn players() -> impl Iterator<Item = Self> {
        (1..=SLOTS).map(Self::Player)
    }

    /// Its name in the mode: `player_<slot>`, or `neutral`.
    pub(crate) fn name(self) -> DeclaredName {
        let name = match self {
            Self::Player(slot) => format!("player_{slot}"),
            Self::Neutral => String::from("neutral"),
        };
        DeclaredName::new(&name).expect("a team's name is a declared name")
    }

    /// The id of the text of its name: `team-player-<slot>`, or `team-neutral`.
    pub(crate) fn text_id(self) -> TextId {
        let id = match self {
            Self::Player(slot) => format!("team-player-{slot}"),
            Self::Neutral => String::from("team-neutral"),
        };
        TextId::new(&id).expect("a team's text id is a message id")
    }

    /// The text of its name, as the mode's language file writes it.
    pub(crate) fn text(self) -> String {
        match self {
            Self::Player(slot) => format!("Player {slot}"),
            Self::Neutral => String::from("Neutral"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_team_has_its_name_and_its_text_and_the_neutral_one_comes_last() {
        let names: Vec<String> = ImportTeam::all()
            .map(|team| team.name().to_string())
            .collect();
        assert_eq!(names.len(), usize::from(SLOTS) + 1);
        assert_eq!(
            [&names[0], &names[7], &names[8]],
            ["player_1", "player_8", "neutral"]
        );
        assert_eq!(ImportTeam::Player(3).text_id().as_str(), "team-player-3");
        assert_eq!(ImportTeam::Neutral.text_id().as_str(), "team-neutral");
        assert_eq!(
            [ImportTeam::Player(3).text(), ImportTeam::Neutral.text()],
            ["Player 3", "Neutral"]
        );
    }
}
