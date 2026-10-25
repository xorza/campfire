use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_common::DeclaredName;
use campfire_math::{Num, Rounding};
use campfire_script::UnknownName;
use campfire_values::{Article, DamageKind, NameId, NameKind, NameScope, Named, Names};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An armor: the share of each damage kind's damage a unit of it takes, by kind, the whole of a
/// kind it does not name, as Warcraft III's table of attack and armor types holds it
/// (04-capabilities/combat.md, Armor by damage kind). Each unit type of the armor shares its one
/// table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Armor(Arc<[Num]>);

/// A share of damage an armor takes, as data writes it: a number at least 0, of any size, as a
/// 200 % weakness is 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmorShare(Num);

impl Armor {
    /// The armor of `shares`, by the damage kinds of `kinds`; an error for a kind `kinds` does not
    /// hold.
    pub fn of(
        shares: &BTreeMap<DeclaredName, ArmorShare>,
        kinds: &Names<DamageKind>,
    ) -> Result<Self, UnknownName> {
        let mut table = vec![Num::ONE; kinds.len()];
        for (kind, share) in shares {
            let kind = kinds
                .named(kind.as_str())
                .ok_or_else(|| UnknownName::new(DamageKind::NAMES, kind))?;
            table[kind.index()] = share.0;
        }
        Ok(Self(table.into()))
    }

    /// What a unit of it takes of `amount` of `kind`, a kind of the mode it was resolved by: the
    /// amount times its share, exactly, rounded down; a product past the range of numbers is past
    /// any life, and takes the most there is.
    pub fn take(&self, kind: DamageKind, amount: Num) -> Num {
        let share = self.0[kind.index()];
        amount
            .checked_mul_div(share, Num::ONE, Rounding::Floor)
            .unwrap_or(Num::MAX)
    }
}

impl ArmorShare {
    /// The share `share`; none below 0.
    pub fn new(share: Num) -> Option<Self> {
        (share >= Num::ZERO).then_some(Self(share))
    }
}

/// As its number, which `Deserialize` reads.
impl Serialize for ArmorShare {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A package is untrusted, so a share below 0, which would heal by damage, fails to read.
impl<'de> Deserialize<'de> for ArmorShare {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let share = Num::deserialize(deserializer)?;
        Self::new(share).ok_or_else(|| D::Error::custom("an armor's share is at least 0"))
    }
}

impl Named for Armor {
    const NAMES: NameKind = NameKind::new("armor", Article::An, NameScope::Match);
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn an_armor_takes_each_kind_at_its_share_rounded_down_and_the_whole_of_others() {
        #[derive(Debug, Deserialize)]
        struct File {
            armor: BTreeMap<DeclaredName, ArmorShare>,
        }
        let read = |text: &str| Toml::parse::<File>(text).map(|file| file.armor);
        let kinds: Names<DamageKind> = ["small_arms", "crush", "explosion"].into_iter().collect();
        let shares = read(r#"armor = { small_arms = "0.5", crush = "2" }"#).unwrap();
        let armor = Armor::of(&shares, &kinds).unwrap();
        let [arms, crush, explosion] = [0, 1, 2].map(DamageKind::new);
        // 5 small arms at 0.5 are 2.5; 5 crush at 2 are 10; a kind it does not name, whole.
        let five = Num::int(5);
        assert_eq!(armor.take(arms, five), Num::int(5) / Num::int(2));
        assert_eq!(armor.take(crush, five), Num::int(10));
        assert_eq!(armor.take(explosion, five), five);
        // The least amount at 0.5 is half a step, rounded down to 0; the most at 2 is past the
        // range, and takes the most there is.
        assert_eq!(armor.take(arms, Num::EPSILON), Num::ZERO);
        assert_eq!(armor.take(crush, Num::MAX), Num::MAX);
        // A share below 0 does not read; a kind the mode does not declare does not resolve.
        let refused = read(r#"armor = { crush = "-0.5" }"#).unwrap_err();
        assert_eq!(refused.message(), "an armor's share is at least 0");
        let stranger = read(r#"armor = { laser = "0" }"#).unwrap();
        assert_eq!(
            Armor::of(&stranger, &kinds),
            Err(UnknownName::new(
                DamageKind::NAMES,
                &DeclaredName::new("laser").unwrap()
            ))
        );
    }
}
