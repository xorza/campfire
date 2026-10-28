use std::collections::BTreeMap;
use std::num::NonZeroU8;

use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;
use crate::hud_file::panel_grid::PanelGrid;
use crate::panel_icon::PanelIcon;

/// A mode's `client/perks.toml`: the choice whose value names a player's pages of perks, the grid
/// each page lays its places on, and for each value of the choice its pages, each of its perks at a
/// place, with its icon (02-engine-core.md, The perk panel by a choice). A mode of no file shows no
/// perk panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerksFile {
    pub choice: DeclaredName,
    #[serde(default)]
    pub grid: PanelGrid,
    #[serde(default)]
    pub pages: BTreeMap<DeclaredName, Vec<PerkPage>>,
}

/// A page of perks: each perk at its place, in their order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerkPage {
    pub places: Vec<PerkPlace>,
}

/// A perk at a place of its page, from 1, with its icon, none for a perk of its name alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerkPlace {
    pub place: NonZeroU8,
    pub perk: DeclaredName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PanelIcon>,
}

impl PerksFile {
    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/perks.toml")
    }

    /// The pages of the player who chose `value`; none for a value of no pages.
    pub fn pages(&self, value: &DeclaredName) -> &[PerkPage] {
        self.pages.get(value).map_or(&[], Vec::as_slice)
    }
}

impl ClientFile for PerksFile {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_choice_s_value_names_its_pages_of_perks_at_their_places() {
        let text = "choice = \"faction\"\n\n[[pages.america]]\n\
                    places = [{ place = 1, perk = \"paladin\" }, { place = 4, perk = \"drop\" }]\n";
        let file = Toml::parse::<PerksFile>(text).unwrap();
        let name = |text: &str| DeclaredName::new(text).unwrap();
        let place = |at, perk: &str| PerkPlace {
            place: NonZeroU8::new(at).unwrap(),
            perk: name(perk),
            icon: None,
        };
        assert_eq!(file.choice, name("faction"));
        assert_eq!(
            file.pages(&name("america")),
            [PerkPage {
                places: vec![place(1, "paladin"), place(4, "drop")],
            }]
        );
        assert_eq!(file.pages(&name("china")), []);
        assert_eq!(Toml::parse::<PerksFile>(&Toml::write(&file)).unwrap(), file);
        assert!(Toml::parse::<PerksFile>("pages = {}").is_err());
    }
}
