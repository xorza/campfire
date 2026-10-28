use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The special powers of Zero Hour's `SpecialPower.ini` files, as `SpecialPowerStore` reads them,
/// by their names, of their case.
#[derive(Debug, Default)]
pub(crate) struct SpecialPowerIni {
    powers: BTreeMap<String, IniSpecialPower>,
}

/// What the import reads of a power: the milliseconds it takes to be ready again, `ReloadTime`, 0
/// by default; the science its player needs for it, `RequiredScience`, none by default or of
/// `NONE`; and whether its player keeps one timer for it, `SharedSyncedTimer`, no by default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IniSpecialPower {
    pub(crate) reload_ms: u32,
    pub(crate) science: Option<String>,
    pub(crate) shared: bool,
}

impl SpecialPowerIni {
    /// Reads one INI file, adding its powers to those read before: a power read again keeps the
    /// fields it does not name.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("specialpower"))
        {
            let block = block?;
            let power = self.powers.entry(block.name()?.to_owned()).or_default();
            for line in &block.fields {
                match line.word() {
                    "reloadtime" => {
                        power.reload_ms = u32::try_from(line.int()?)
                            .map_err(|_| IniError::Range { line: line.number })?;
                    }
                    "requiredscience" => {
                        let value = line.value()?;
                        power.science =
                            (!value.eq_ignore_ascii_case("none")).then(|| value.to_owned());
                    }
                    "sharedsyncedtimer" => power.shared = line.yes()?,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The power `name`, of its case; none for one no file defines.
    pub(crate) fn named(&self, name: &str) -> Option<&IniSpecialPower> {
        self.powers.get(name)
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\SpecialPower.ini` of the fixture install: Emergency Repair.
    pub(crate) const SPECIAL_POWERS: &str = "SpecialPower SuperweaponEmergencyRepair\r
  Enum = SPECIAL_REPAIR_VEHICLES\r
  ReloadTime = 240000 ; in milliseconds\r
  RequiredScience = SCIENCE_EmergencyRepair1\r
  SharedSyncedTimer = Yes\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::special_power_ini::samples::SPECIAL_POWERS;

    #[test]
    fn a_power_takes_its_reload_its_science_and_its_shared_timer() {
        let mut ini = SpecialPowerIni::default();
        ini.read(SPECIAL_POWERS.as_bytes()).unwrap();
        assert_eq!(
            ini.named("SuperweaponEmergencyRepair"),
            Some(&IniSpecialPower {
                reload_ms: 240_000,
                science: Some("SCIENCE_EmergencyRepair1".to_owned()),
                shared: true,
            })
        );
        ini.read(b"SpecialPower SuperweaponEmergencyRepair\n RequiredScience = NONE\nEnd\n")
            .unwrap();
        let power = ini.named("SuperweaponEmergencyRepair").unwrap();
        assert_eq!((power.reload_ms, &power.science), (240_000, &None));
        let wrong = b"SpecialPower A\n ReloadTime = -1\nEnd\n";
        assert_eq!(
            SpecialPowerIni::default().read(wrong),
            Err(IniError::Range { line: 2 })
        );
    }
}
