use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use campfire_values::Stat;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A unit type's `income` section: what it pays its owner in `resource` each `period_ms` from its
/// spawn, while the unit is owned and finished, and what a capture pays once after its first
/// period, 0 by default (04-capabilities/games.md, Income over time).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IncomeData {
    pub resource: DeclaredName,
    #[serde(flatten)]
    pub pays: IncomePays,
    pub period_ms: u64,
    pub capture: u32,
}

/// What a period pays: a fixed `amount`, or the unit's stat `amount_stat`, rounded down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IncomePays {
    Amount(u32),
    AmountStat(Stat),
}

/// Data is untrusted, so an income of both or neither of an amount and a stat, or of a period of
/// no time, fails to read.
impl<'de> Deserialize<'de> for IncomeData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            resource: DeclaredName,
            amount: Option<u32>,
            amount_stat: Option<Stat>,
            period_ms: u64,
            #[serde(default)]
            capture: u32,
        }
        let Fields {
            resource,
            amount,
            amount_stat,
            period_ms,
            capture,
        } = Fields::deserialize(deserializer)?;
        let pays = match (amount, amount_stat) {
            (Some(amount), None) => IncomePays::Amount(amount),
            (None, Some(stat)) => IncomePays::AmountStat(stat),
            _ => {
                return Err(D::Error::custom(
                    "an income pays an amount or an amount stat",
                ));
            }
        };
        if period_ms == 0 {
            return Err(D::Error::custom("an income's period is past 0 ms"));
        }
        Ok(Self {
            resource,
            pays,
            period_ms,
            capture,
        })
    }
}

impl DataSection for IncomeData {
    const TABLE: DataTable = DataTable::new("A unit type's `income`");
}

impl Section for IncomeData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "income";
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn an_income_pays_an_amount_or_a_stat_each_period_past_0() {
        let read = |text: &str| Toml::parse::<IncomeData>(text);
        let paid = read("resource = \"money\"\namount = 20\nperiod_ms = 2000").unwrap();
        assert_eq!(
            (&paid.pays, paid.period_ms, paid.capture),
            (&IncomePays::Amount(20), 2000, 0)
        );
        let stat =
            read("resource = \"money\"\namount_stat = \"supply\"\nperiod_ms = 1\ncapture = 5");
        let stat = stat.unwrap();
        assert_eq!(
            (stat.pays, stat.capture),
            (IncomePays::AmountStat(Stat::named("supply").unwrap()), 5)
        );
        for refused in [
            "resource = \"money\"\nperiod_ms = 1",
            "resource = \"money\"\namount = 2\namount_stat = \"supply\"\nperiod_ms = 1",
            "resource = \"money\"\namount = 2\nperiod_ms = 0",
            "resource = \"money\"\namount = 2",
        ] {
            assert!(read(refused).is_err(), "{refused}");
        }
        // As the import writes one, it reads back.
        assert_eq!(read(&Toml::write(&paid)).unwrap(), paid);
    }
}
