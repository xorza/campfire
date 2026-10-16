use std::collections::BTreeMap;

use crate::zero_hour::format::ini::damage_type::DamageType;
use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniLine, IniText};

/// The armors of Zero Hour's `Armor.ini` files, as `ArmorStore` reads them, by their names, of
/// their case.
#[derive(Debug, Default)]
pub(crate) struct ArmorIni {
    armors: BTreeMap<String, IniArmor>,
}

/// An armor's share of each damage type's damage, by the type's place in `DamageType::ALL`, as its
/// lines set them in order: 1 for a type no line names, and a line of `DEFAULT` setting every
/// type at once (`ArmorTemplate::parseArmorCoefficients`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniArmor {
    shares: [f32; DamageType::ALL.len()],
}

impl Default for IniArmor {
    fn default() -> Self {
        Self {
            shares: [1.0; DamageType::ALL.len()],
        }
    }
}

impl ArmorIni {
    /// Reads one INI file, adding its armors to those read before: one read again keeps its
    /// shares and takes the new ones.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).blocks(BlockKinds::One("armor")) {
            let block = block?;
            let armor = self.armors.entry(block.name()?.to_owned()).or_default();
            for line in block.fields.iter().filter(|line| line.word() == "armor") {
                armor.share_line(line)?;
            }
        }
        Ok(())
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
    /// Its share of the damage type `kind`: 1 for a type the game passes whole.
    pub(crate) const fn share(&self, kind: DamageType) -> f32 {
        if kind.resisted() {
            self.shares[kind.index()]
        } else {
            1.0
        }
    }

    /// Takes an `Armor = <type> <percent>` line: the type of either case, or `DEFAULT` for every
    /// type, and its share, the percent's number over 100, as `INI::scanPercentToReal` reads it;
    /// an error for a type the game does not name.
    fn share_line(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        let name = at.value()?;
        let percent = at.real_at(2)? / 100.0;
        if name.eq_ignore_ascii_case("default") {
            self.shares = [percent; DamageType::ALL.len()];
            return Ok(());
        }
        let kind = DamageType::of(name).ok_or(IniError::NotListed { line: at.number })?;
        self.shares[kind.index()] = percent;
        Ok(())
    }
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
        let get = |name| ini.get(name).unwrap();
        let human = get("HumanArmor");
        assert_eq!(human.share(DamageType::Crush), 2.0);
        assert_eq!(human.share(DamageType::SmallArms), 1.0);
        // `DEFAULT` sets every type, then small arms, of either case, a quarter; `UNRESISTABLE`
        // and `SUBDUAL_UNRESISTABLE` pass whole whatever the armor says.
        let building = get("StructureArmor");
        assert_eq!(building.share(DamageType::Explosion), 0.5);
        assert_eq!(building.share(DamageType::SmallArms), 0.25);
        assert_eq!(building.share(DamageType::Unresistable), 1.0);
        assert_eq!(building.share(DamageType::SubdualUnresistable), 1.0);
        assert!(ini.get("humanarmor").is_none());
        // A type the game does not name is refused.
        let unknown = b"Armor Odd\r\n  Armor = FROST 50%\r\nEnd\r\n";
        assert!(ArmorIni::default().read(unknown).is_err());
    }
}
