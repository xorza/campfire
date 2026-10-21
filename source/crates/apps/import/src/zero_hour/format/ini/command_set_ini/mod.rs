use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The command sets of Zero Hour's `CommandSet.ini` files, as `ControlBar` reads them, by their
/// names, of their case: each set's buttons by their places, 1 to 18.
#[derive(Debug, Default)]
pub(crate) struct CommandSetIni {
    sets: BTreeMap<String, BTreeMap<u8, String>>,
}

impl CommandSetIni {
    /// The places of a set, `MAX_COMMANDS_PER_SET`.
    const PLACES: u8 = 18;

    /// Reads one INI file, adding its sets to those read before: a set read again keeps the
    /// places it does not name and takes the new ones, as an override does.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).blocks(BlockKinds::One("commandset")) {
            let block = block?;
            let set = self.sets.entry(block.name()?.to_owned()).or_default();
            for line in &block.fields {
                // A place's field is named by its number, `1` to `18`.
                let place = line
                    .word()
                    .parse::<u8>()
                    .ok()
                    .filter(|place| (1..=Self::PLACES).contains(place))
                    .ok_or(IniError::NotListed { line: line.number })?;
                set.insert(place, line.value()?.to_owned());
            }
        }
        Ok(())
    }

    /// The buttons of the set `name`, in the order of their places; none for a name no file
    /// defines.
    pub(crate) fn buttons(&self, name: &str) -> Option<impl Iterator<Item = &str>> {
        Some(self.sets.get(name)?.values().map(String::as_str))
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\CommandSet.ini` of the fixture install: the command center's, the dozer's and
    /// the barracks', each of a button the import drops among those it converts.
    pub(crate) const COMMAND_SETS: &str = "CommandSet AmericaCommandCenterCommandSet\r
  1 = Command_ConstructAmericaDozer\r
  13 = Command_SetRallyPoint\r
End\r
CommandSet AmericaDozerCommandSet\r
  3 = Command_ConstructAmericaBarracks\r
  1 = Command_ConstructAmericaPowerPlant ; a science\r
  4 = Command_ConstructAmericaFireBase ; no box\r
  5 = Command_ConstructAmericaInfantryRanger ; no queue to train at\r
End\r
CommandSet AmericaBarracksCommandSet\r
  1 = Command_ConstructAmericaInfantryRanger\r
  2 = Command_ConstructAmericaInfantryPathfinder ; either of two\r
  3 = Command_ConstructFortressWall ; a train of no walker\r
  5 = Command_ConstructGhost ; no object\r
  6 = Command_ConstructAmericaPowerPlant ; dropped again, reported once\r
End\r
";
}

#[cfg(test)]
mod tests;
