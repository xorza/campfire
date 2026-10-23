use campfire_values::{PackagePath, Team};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::error::TeamColorsError;

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

    /// The colors `colors`; an error for no color, or for more than `MOST`.
    pub fn new(colors: Vec<[u8; 3]>) -> Result<Self, TeamColorsError> {
        if colors.is_empty() {
            return Err(TeamColorsError::None);
        }
        if colors.len() > Self::MOST {
            return Err(TeamColorsError::TooMany(colors.len()));
        }
        Ok(Self { colors })
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
        Self::new(colors).map_err(D::Error::custom)
    }
}

impl ClientFile for TeamColors {
    const FORMAT: ClientFormat = ClientFormat::Toml;
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
        let refusal = |text: &str| {
            let refused = Toml::parse::<TeamColors>(text).unwrap_err();
            refused.message().to_owned()
        };
        assert_eq!(refusal("colors = []"), "it gives no color");
        let many = format!("colors = [{}]", vec!["[1, 2, 3]"; 257].join(", "));
        assert_eq!(refusal(&many), "it gives 257 colors, more than 256");
        assert_eq!(TeamColors::new(Vec::new()), Err(TeamColorsError::None));
        assert_eq!(
            TeamColors::new(vec![[1, 2, 3]; 257]),
            Err(TeamColorsError::TooMany(257))
        );
        assert_eq!(
            refusal("colors = [[256, 0, 0]]"),
            "invalid value: integer `256`, expected u8"
        );
        assert_eq!(TeamColors::default().colors().len(), 3);
    }
}
