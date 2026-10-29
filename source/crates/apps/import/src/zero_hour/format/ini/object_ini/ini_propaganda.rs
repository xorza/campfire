use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// An object's `PropagandaTowerBehavior`, as its lines write it: the radius it reaches,
/// `Radius`; the share of each unit's most health it heals a second, `HealPercentEachSecond`, and
/// once its player holds `UpgradeRequired`, `UpgradedHealPercentEachSecond`, each a share as
/// `INI::parsePercentToReal` reads it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniPropaganda {
    pub(crate) radius: f32,
    pub(crate) heal: f32,
    pub(crate) upgraded_heal: f32,
    pub(crate) upgrade: Option<String>,
}

impl IniPropaganda {
    /// Reads a line of the module: whether it is one of its fields.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        match word {
            "radius" => self.radius = at.real()?,
            "healpercenteachsecond" => self.heal = at.real()? / 100.0,
            "upgradedhealpercenteachsecond" => self.upgraded_heal = at.real()? / 100.0,
            "upgraderequired" => self.upgrade = Some(at.value()?.to_owned()),
            _ => return Ok(false),
        }
        Ok(true)
    }
}

/// `PropagandaTowerBehaviorModuleData`'s: a radius of 1, and 1 % and 2 % a second.
impl Default for IniPropaganda {
    fn default() -> Self {
        Self {
            radius: 1.0,
            heal: 0.01,
            upgraded_heal: 0.02,
            upgrade: None,
        }
    }
}
