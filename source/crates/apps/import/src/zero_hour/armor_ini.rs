use std::collections::BTreeMap;

use crate::zero_hour::error::IniError;
use crate::zero_hour::ini_text::{IniLine, IniText};

/// Zero Hour's damage types, in the order of `DamageTypeFlags`' names, as the game's INI writes
/// them: Generals' `FLESHY_SNIPER` is not among them.
pub(crate) const DAMAGE_TYPES: [&str; 38] = [
    "EXPLOSION",
    "CRUSH",
    "ARMOR_PIERCING",
    "SMALL_ARMS",
    "GATTLING",
    "RADIATION",
    "FLAME",
    "LASER",
    "SNIPER",
    "POISON",
    "HEALING",
    "UNRESISTABLE",
    "WATER",
    "DEPLOY",
    "SURRENDER",
    "HACK",
    "KILL_PILOT",
    "PENALTY",
    "FALLING",
    "MELEE",
    "DISARM",
    "HAZARD_CLEANUP",
    "PARTICLE_BEAM",
    "TOPPLING",
    "INFANTRY_MISSILE",
    "AURORA_BOMB",
    "LAND_MINE",
    "JET_MISSILES",
    "STEALTHJET_MISSILES",
    "MOLOTOV_COCKTAIL",
    "COMANCHE_VULCAN",
    "SUBDUAL_MISSILE",
    "SUBDUAL_VEHICLE",
    "SUBDUAL_BUILDING",
    "SUBDUAL_UNRESISTABLE",
    "MICROWAVE",
    "KILL_GARRISONED",
    "STATUS",
];

/// The damage types `ArmorTemplate::adjustDamage` passes whole, whatever an armor says of them.
const UNRESISTED: [usize; 2] = [11, 34];

/// The armors of Zero Hour's `Armor.ini` files, as `ArmorStore` reads them, by their names, of
/// their case.
#[derive(Debug, Default)]
pub(crate) struct ArmorIni {
    armors: BTreeMap<String, IniArmor>,
}

/// An armor's share of each damage type's damage, by the type's place in `DAMAGE_TYPES`, as its
/// lines set them in order: 1 for a type no line names, and a line of `DEFAULT` setting every
/// type at once (`ArmorTemplate::parseArmorCoefficients`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniArmor {
    shares: [f32; DAMAGE_TYPES.len()],
}

impl Default for IniArmor {
    fn default() -> IniArmor {
        IniArmor {
            shares: [1.0; DAMAGE_TYPES.len()],
        }
    }
}

impl ArmorIni {
    /// Reads one INI file, adding its armors to those read before: one read again keeps its
    /// shares and takes the new ones.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut open: Option<String> = None;
        for line in text.lines() {
            let word = line.word();
            match &open {
                None if word == "armor" => {
                    let name = *line
                        .tokens
                        .get(1)
                        .ok_or(IniError::NoName { line: line.number })?;
                    self.armors.entry(name.to_owned()).or_default();
                    open = Some(name.to_owned());
                }
                None if word == "end" => return Err(IniError::StrayEnd { line: line.number }),
                None => return Err(IniError::NotBlock { line: line.number }),
                Some(_) if word == "end" => open = None,
                Some(name) if word == "armor" => self
                    .armors
                    .get_mut(name)
                    .expect("an open armor is in the list")
                    .share_line(&line)?,
                Some(_) => {}
            }
        }
        match open {
            None => Ok(()),
            Some(_) => Err(IniError::Unclosed),
        }
    }

    /// The armor `name`, of its case, as `ArmorStore::findArmorTemplate` finds it; none for a
    /// name no file defines.
    pub(crate) fn get(&self, name: &str) -> Option<&IniArmor> {
        self.armors.get(name)
    }

    /// Each armor, by its name.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &IniArmor)> {
        self.armors
            .iter()
            .map(|(name, armor)| (name.as_str(), armor))
    }
}

impl IniArmor {
    /// Its share of the damage type at `at` in `DAMAGE_TYPES`: 1 for a type the game passes
    /// whole.
    pub(crate) fn share(&self, at: usize) -> f32 {
        if UNRESISTED.contains(&at) {
            1.0
        } else {
            self.shares[at]
        }
    }

    /// Takes an `Armor = <type> <percent>` line: the type of either case, or `DEFAULT` for every
    /// type, and its share, the percent's number over 100, as `INI::scanPercentToReal` reads it;
    /// an error for a type the game does not name.
    fn share_line(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        let name = at.value()?;
        let percent = at.real_at(2)? / 100.0;
        if name.eq_ignore_ascii_case("default") {
            self.shares = [percent; DAMAGE_TYPES.len()];
            return Ok(());
        }
        let place = damage_type(name).ok_or(IniError::NotListed { line: at.number })?;
        self.shares[place] = percent;
        Ok(())
    }
}

/// The place in `DAMAGE_TYPES` of the type `name`, of either case, as `INI::scanIndexList` finds
/// it.
pub(crate) fn damage_type(name: &str) -> Option<usize> {
    DAMAGE_TYPES
        .iter()
        .position(|known| name.eq_ignore_ascii_case(known))
}

#[cfg(test)]
pub(crate) mod internals {
    /// `Data\INI\Armor.ini` of the fixture install: a soldier's armor, which small arms hit whole
    /// and crushing twice; and a building's, every type at half by `DEFAULT`, small arms at a
    /// quarter after it, and none of `UNRESISTABLE`, which still passes whole.
    pub(crate) const ARMORS: &str = "Armor HumanArmor\r
  Armor = CRUSH 200%\r
  Armor = SNIPER 200%\r
End\r
Armor StructureArmor\r
  Armor = DEFAULT 50%\r
  Armor = small_arms 25%\r
  Armor = UNRESISTABLE 0%\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_armor_shares_each_type_by_its_lines_in_order_and_passes_the_unresisted_whole() {
        let mut ini = ArmorIni::default();
        ini.read(internals::ARMORS.as_bytes()).unwrap();
        let place = |name| damage_type(name).unwrap();
        let get = |name| ini.get(name).unwrap();
        let human = get("HumanArmor");
        assert_eq!(human.share(place("CRUSH")), 2.0);
        assert_eq!(human.share(place("SMALL_ARMS")), 1.0);
        // `DEFAULT` sets every type, then small arms, of either case, a quarter; `UNRESISTABLE`
        // and `SUBDUAL_UNRESISTABLE` pass whole whatever the armor says.
        let building = get("StructureArmor");
        assert_eq!(building.share(place("EXPLOSION")), 0.5);
        assert_eq!(building.share(place("small_arms")), 0.25);
        assert_eq!(building.share(place("UNRESISTABLE")), 1.0);
        assert_eq!(building.share(place("SUBDUAL_UNRESISTABLE")), 1.0);
        assert_eq!(
            [UNRESISTED[0], UNRESISTED[1]],
            [place("UNRESISTABLE"), place("SUBDUAL_UNRESISTABLE")]
        );
        assert!(ini.get("humanarmor").is_none());
        // A type the game does not name is refused.
        let unknown = b"Armor Odd\r\n  Armor = FROST 50%\r\nEnd\r\n";
        assert!(ArmorIni::default().read(unknown).is_err());
        assert_eq!(DAMAGE_TYPES.len(), 38);
    }
}
