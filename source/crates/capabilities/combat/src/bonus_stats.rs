use campfire_math::Num;
use campfire_units::StatId;
use campfire_values::{NameId, Stat};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A weapon's `rate_bonus` or `damage_bonus` as data writes it: a stat, or a list of them, whose
/// sum is the bonus, as Zero Hour's `WeaponBonusSet::appendBonuses` adds each condition's bonus
/// (generals.md, A weapon's bonus is a sum of stats).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BonusData(Vec<Stat>);

/// A weapon's bonus stats, resolved; one at least.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BonusStats(Box<[StatId]>);

impl BonusData {
    /// A bonus of `stats`; `None` for none.
    pub fn of(stats: impl IntoIterator<Item = Stat>) -> Option<Self> {
        let stats: Vec<Stat> = stats.into_iter().collect();
        (!stats.is_empty()).then_some(Self(stats))
    }

    /// Its stats, one at least.
    pub fn stats(&self) -> &[Stat] {
        &self.0
    }
}

impl From<Stat> for BonusData {
    fn from(stat: Stat) -> Self {
        Self(vec![stat])
    }
}

impl FromIterator<StatId> for BonusStats {
    fn from_iter<I: IntoIterator<Item = StatId>>(stats: I) -> Self {
        let stats: Box<[StatId]> = stats.into_iter().collect();
        debug_assert!(!stats.is_empty(), "a bonus of one stat at least");
        Self(stats)
    }
}

impl<const N: usize> From<[StatId; N]> for BonusStats {
    fn from(stats: [StatId; N]) -> Self {
        stats.into_iter().collect()
    }
}

impl BonusStats {
    /// The bonus of a unit of stats `stats`: the sum of its stats' values, each 0 that the unit
    /// does not hold yet, exact and saturating past the range of numbers.
    pub(crate) fn value(&self, stats: &[Num]) -> Num {
        let bits = self.0.iter().fold(0_i128, |sum, stat| {
            let value = stats.get(stat.index()).copied().unwrap_or(Num::ZERO);
            sum + i128::from(value.to_bits())
        });
        Num::saturating_from_bits(bits)
    }
}

/// Data is untrusted, so an empty list fails to read.
impl<'de> Deserialize<'de> for BonusData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            One(Stat),
            Many(Vec<Stat>),
        }
        match Written::deserialize(deserializer)? {
            Written::One(stat) => Ok(Self::from(stat)),
            Written::Many(stats) => {
                Self::of(stats).ok_or_else(|| D::Error::custom("a bonus of one stat at least"))
            }
        }
    }
}

/// As data writes it: one stat alone, several as a list.
impl Serialize for BonusData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0.as_slice() {
            [stat] => stat.serialize(serializer),
            stats => stats.serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_bonus_is_a_stat_or_a_list_and_its_value_their_sum() {
        #[derive(Debug, Deserialize)]
        struct Holder {
            bonus: BonusData,
        }
        let read = |text: &str| Toml::parse::<Holder>(text).map(|holder| holder.bonus);
        let stat = |name| Stat::named(name).unwrap();
        assert_eq!(
            read("bonus = \"rate_of_fire\"").unwrap(),
            BonusData::from(stat("rate_of_fire"))
        );
        assert_eq!(
            read("bonus = [\"rate_of_fire\", \"veteran_rate\"]")
                .unwrap()
                .stats(),
            [stat("rate_of_fire"), stat("veteran_rate")]
        );
        assert!(read("bonus = []").is_err());
        // 0.25 and 0.5, and a third stat the unit does not hold: 0.75.
        let bonus = BonusStats::from([0, 1, 5].map(StatId::new));
        let stats = [Num::QUARTER, Num::HALF];
        assert_eq!(bonus.value(&stats), Num::HALF + Num::QUARTER);
        assert_eq!(
            BonusStats::from([StatId::new(0)]).value(&[Num::MAX, Num::ONE]),
            Num::MAX
        );
    }
}
