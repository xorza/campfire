use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniLine, IniText};

/// The player templates of Zero Hour's `PlayerTemplate.ini` files, as `PlayerTemplateStore`
/// reads them, by their names, of their case, as `NAMEKEY` finds them, and their names in the
/// order the files first define them, the store's order, which its lobby lists them in.
#[derive(Debug, Default)]
pub(crate) struct PlayerTemplateIni {
    templates: BTreeMap<String, PlayerTemplate>,
    order: Vec<String>,
}

/// What the import reads of a player template: whether a player may play it, its
/// `PlayableSide`, and the objects a skirmish starts its player with, its `StartingBuilding` and
/// its `StartingUnit0` to `StartingUnit9`, each none or no when absent, as the template starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PlayerTemplate {
    pub(crate) playable: bool,
    pub(crate) building: Option<String>,
    units: [Option<String>; Self::UNITS],
}

impl PlayerTemplateIni {
    /// Reads one INI file, adding its templates to those read before: one read again keeps its
    /// fields and takes the new ones, as a template found again is read into.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).blocks(BlockKinds::One("playertemplate")) {
            let block = block?;
            let name = block.name()?;
            if !self.templates.contains_key(name) {
                self.order.push(name.to_owned());
            }
            let template = self.templates.entry(name.to_owned()).or_default();
            for line in &block.fields {
                template.field(&line.word(), line)?;
            }
        }
        Ok(())
    }

    /// The templates a player may play, by name, in the store's order.
    pub(crate) fn playable(&self) -> impl Iterator<Item = (&str, &PlayerTemplate)> {
        self.order
            .iter()
            .map(|name| (name.as_str(), &self.templates[name]))
            .filter(|(_, template)| template.playable)
    }
}

impl PlayerTemplate {
    /// `MAX_MP_STARTING_UNITS`.
    const UNITS: usize = 10;

    /// The objects it starts its player with beside its building, in the order of their fields.
    pub(crate) fn units(&self) -> impl Iterator<Item = &str> {
        self.units.iter().flatten().map(String::as_str)
    }

    /// Takes the line of `word`, a field the import reads or another.
    fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        // `StartingUnit0` to `StartingUnit9`: one digit for each of the ten.
        let unit = word
            .strip_prefix("startingunit")
            .filter(|digit| digit.len() == 1)
            .and_then(|digit| digit.parse::<usize>().ok());
        match (word, unit) {
            ("playableside", _) => self.playable = at.yes()?,
            ("startingbuilding", _) => self.building = Some(at.value()?.to_owned()),
            (_, Some(index)) => self.units[index] = Some(at.value()?.to_owned()),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\PlayerTemplate.ini` of the fixture install: the observer's, which no player
    /// plays and starts with nothing, and America's, whose skirmish starts it with a command
    /// center and a dozer.
    pub(crate) const PLAYER_TEMPLATES: &str = "PlayerTemplate FactionObserver\r
  Side = Observer\r
  PlayableSide = No\r
End\r
PlayerTemplate FactionAmerica\r
  Side = America\r
  PlayableSide = Yes\r
  StartingBuilding = AmericaCommandCenter\r
  StartingUnit0 = AmericaVehicleDozer ; the builder\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::player_template_ini::samples::PLAYER_TEMPLATES;

    #[test]
    fn a_template_takes_its_start_objects_a_later_file_last() {
        let mut ini = PlayerTemplateIni::default();
        ini.read(PLAYER_TEMPLATES.as_bytes()).unwrap();
        let america = &ini.templates["FactionAmerica"];
        assert_eq!(america.building.as_deref(), Some("AmericaCommandCenter"));
        assert_eq!(america.units().collect::<Vec<_>>(), ["AmericaVehicleDozer"]);
        assert_eq!(ini.templates["FactionObserver"], PlayerTemplate::default());
        // Only America is played.
        let played: Vec<&str> = ini.playable().map(|(name, _)| name).collect();
        assert_eq!(played, ["FactionAmerica"]);
        // A later file keeps the fields it does not name, and fills a unit's place by its
        // number; a field past the tenth unit, or of another case's template, is none of them.
        ini.read(
            b"PlayerTemplate FactionAmerica\n startingunit3 = Ranger\n StartingUnit10 = Tank\nEnd\n",
        )
        .unwrap();
        let america = &ini.templates["FactionAmerica"];
        assert_eq!(america.building.as_deref(), Some("AmericaCommandCenter"));
        assert_eq!(
            america.units().collect::<Vec<_>>(),
            ["AmericaVehicleDozer", "Ranger"]
        );
        assert!(!ini.templates.contains_key("factionamerica"));

        let fails = |text: &[u8]| PlayerTemplateIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"PlayerTemplate A\n StartingBuilding\nEnd\n"),
            IniError::NoValue { line: 2 }
        );
        assert_eq!(fails(b"Side = America\n"), IniError::NotBlock { line: 1 });
        assert_eq!(fails(b"End\n"), IniError::StrayEnd { line: 1 });
        assert_eq!(
            fails(b"PlayerTemplate\nEnd\n"),
            IniError::NoName { line: 1 }
        );
        assert_eq!(fails(b"PlayerTemplate A\n"), IniError::Unclosed);
    }
}
