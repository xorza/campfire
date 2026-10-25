use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The upgrades of Zero Hour's `Upgrade.ini` files, as `UpgradeCenter` reads them, by their names,
/// of their case.
#[derive(Debug, Default)]
pub(crate) struct UpgradeIni {
    upgrades: BTreeMap<String, IniUpgrade>,
}

/// What the import reads of an upgrade: whom it upgrades, its build time in seconds and its cost,
/// each its default when absent, a player's upgrade of no time and no cost (`UpgradeTemplate`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IniUpgrade {
    pub(crate) kind: UpgradeKind,
    pub(crate) time: f32,
    pub(crate) cost: u16,
}

/// An upgrade's `Type`, of `TheUpgradeTypeNames`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpgradeKind {
    /// `PLAYER`: its player holds it, for each of its objects.
    Player,
    /// `OBJECT`: the object that researched it holds it.
    Object,
}

impl Default for IniUpgrade {
    fn default() -> Self {
        Self {
            kind: UpgradeKind::Player,
            time: 0.0,
            cost: 0,
        }
    }
}

impl UpgradeIni {
    /// Reads one INI file, adding its upgrades to those read before: an upgrade read again keeps
    /// the fields it does not name and takes the new ones, as `parseUpgradeDefinition` reads into
    /// the one it finds. A type of no name the game lists fails the read, as `INI::scanIndexList`
    /// does.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("upgrade"))
        {
            let block = block?;
            let upgrade = self.upgrades.entry(block.name()?.to_owned()).or_default();
            for line in &block.fields {
                match line.word() {
                    "type" => {
                        upgrade.kind = match line.value()?.to_ascii_uppercase().as_str() {
                            "PLAYER" => UpgradeKind::Player,
                            "OBJECT" => UpgradeKind::Object,
                            _ => return Err(IniError::NotListed { line: line.number }),
                        };
                    }
                    "buildtime" => upgrade.time = line.real()?,
                    "buildcost" => {
                        let cost = line.int()?;
                        upgrade.cost = u16::try_from(cost)
                            .map_err(|_| IniError::Range { line: line.number })?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The name of each upgrade the files define, in their order.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.upgrades.keys().map(String::as_str)
    }

    /// The upgrade `name`; none for a name no file defines.
    pub(crate) fn get(&self, name: &str) -> Option<&IniUpgrade> {
        self.upgrades.get(name)
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\Upgrade.ini` of the fixture install: a player upgrade that each of the
    /// crusader's and the ranger's upgrade modules waits for, and an object upgrade the barracks
    /// researches for itself.
    pub(crate) const UPGRADES: &str = "Upgrade Upgrade_AmericaCompositeArmor\r
  DisplayName = UPGRADE:CompositeArmor\r
  Type = PLAYER\r
  BuildTime = 30.0\r
  BuildCost = 1000\r
End\r
Upgrade Upgrade_AmericaRangerFlashBang\r
  BuildTime = 15.5\r
  BuildCost = 500\r
End\r
Upgrade Upgrade_AmericaBarracksWalls\r
  Type = OBJECT\r
  BuildTime = 10.0\r
  BuildCost = 400\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::upgrade_ini::samples::UPGRADES;

    #[test]
    fn an_upgrade_takes_its_type_time_and_cost_a_later_file_over_the_fields_it_names() {
        let mut ini = UpgradeIni::default();
        ini.read(UPGRADES.as_bytes()).unwrap();
        let armor = IniUpgrade {
            kind: UpgradeKind::Player,
            time: 30.0,
            cost: 1000,
        };
        assert_eq!(ini.get("Upgrade_AmericaCompositeArmor"), Some(&armor));
        // No type is a player's.
        let flash = ini.get("Upgrade_AmericaRangerFlashBang").unwrap();
        assert_eq!(
            (flash.kind, flash.time, flash.cost),
            (UpgradeKind::Player, 15.5, 500)
        );
        let walls = ini.get("Upgrade_AmericaBarracksWalls").unwrap();
        assert_eq!(walls.kind, UpgradeKind::Object);
        assert!(ini.get("upgrade_americacompositearmor").is_none());
        // A later file keeps the fields it does not name; a type of either case is read.
        ini.read(b"Upgrade Upgrade_AmericaCompositeArmor\n Type = object\nEnd\n")
            .unwrap();
        let again = ini.get("Upgrade_AmericaCompositeArmor").unwrap();
        assert_eq!(
            again,
            &IniUpgrade {
                kind: UpgradeKind::Object,
                ..armor
            }
        );
        let fails = |text: &[u8]| UpgradeIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"Upgrade A\n Type = TEAM\nEnd\n"),
            IniError::NotListed { line: 2 }
        );
        assert_eq!(
            fails(b"Upgrade A\n BuildCost = 70000\nEnd\n"),
            IniError::Range { line: 2 }
        );
    }
}
