use campfire_values::{PackagePath, Team};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_path;

/// A mode's `client/team_colors.toml`: the color of each team, in sRGB of 8 bits a channel, by
/// the team's index, a team past the list taking the list's colors again from its first; a mode
/// of no file takes the engine's (02-engine-core.md, Team colors in the one material).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TeamColors {
    colors: Vec<[u8; 3]>,
}

impl TeamColors {
    /// The most colors a file gives.
    pub const MOST: usize = 256;

    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/team_colors.toml")
    }

    /// The colors `colors`; none for no color, or for more than `MOST`.
    pub fn new(colors: Vec<[u8; 3]>) -> Option<Self> {
        (!colors.is_empty() && colors.len() <= Self::MOST).then_some(Self { colors })
    }

    pub fn colors(&self) -> &[[u8; 3]] {
        &self.colors
    }

    /// The place in the list of the color of `team`.
    pub fn place(&self, team: Team) -> usize {
        usize::from(team.get()) % self.colors.len()
    }
}

/// The engine's colors: blue, red and gray.
impl Default for TeamColors {
    fn default() -> Self {
        Self {
            colors: vec![[64, 115, 242], [230, 64, 51], [153, 153, 153]],
        }
    }
}

/// A package's file is untrusted, so a list of no color, or of more than `MOST`, fails to read.
impl<'de> Deserialize<'de> for TeamColors {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            colors: Vec<[u8; 3]>,
        }
        let Fields { colors } = Fields::deserialize(deserializer)?;
        Self::new(colors)
            .ok_or_else(|| D::Error::custom("a team colors file gives 1 to 256 colors"))
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_team_takes_its_color_by_its_index_and_past_the_list_again_from_its_first() {
        let colors: TeamColors = Toml::parse("colors = [[255, 198, 0], [221, 0, 0]]").unwrap();
        assert_eq!(colors.colors(), [[255, 198, 0], [221, 0, 0]]);
        let places = [0, 1, 2, 5].map(|team| colors.place(Team::new(team)));
        assert_eq!(places, [0, 1, 0, 1]);
        assert_eq!(
            Toml::parse::<TeamColors>(&Toml::write(&colors)).unwrap(),
            colors
        );
        // No color, more than 256, or a channel past 255, fails to read.
        assert!(Toml::parse::<TeamColors>("colors = []").is_err());
        let many = format!("colors = [{}]", vec!["[1, 2, 3]"; 257].join(", "));
        assert!(Toml::parse::<TeamColors>(&many).is_err());
        assert!(Toml::parse::<TeamColors>("colors = [[256, 0, 0]]").is_err());
        assert_eq!(TeamColors::default().colors().len(), 3);
    }
}
