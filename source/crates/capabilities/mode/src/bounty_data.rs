use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use campfire_values::FilterData;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// The mode's `bounty` section: the killers whose kills pay, a filter from the victim's side, and
/// the share of a victim's cost each player starts with, 0 or more (04-capabilities/games.md, A
/// kill's bounty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeBountyData {
    pub killer: FilterData,
    pub share: Num,
}

/// A unit type's `bounty` section: what its death pays the owner of its killer, in `resource`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BountyData {
    pub resource: DeclaredName,
    #[serde(flatten)]
    pub pays: Pays,
}

/// What a death pays: a fixed `amount`, or the killer's player's share of the victim's `cost`,
/// rounded up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Pays {
    Amount(u32),
    Cost(u32),
}

/// Data is untrusted, so a share below 0 fails to read.
impl<'de> Deserialize<'de> for ModeBountyData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            killer: FilterData,
            #[serde(default)]
            share: Num,
        }
        let Fields { killer, share } = Fields::deserialize(deserializer)?;
        if share < Num::ZERO {
            return Err(D::Error::custom("a bounty's share is 0 or more"));
        }
        Ok(Self { killer, share })
    }
}

/// A type's bounty gives an `amount` or a `cost`, and not both.
impl<'de> Deserialize<'de> for BountyData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            resource: DeclaredName,
            amount: Option<u32>,
            cost: Option<u32>,
        }
        let Fields {
            resource,
            amount,
            cost,
        } = Fields::deserialize(deserializer)?;
        let pays = match (amount, cost) {
            (Some(amount), None) => Pays::Amount(amount),
            (None, Some(cost)) => Pays::Cost(cost),
            _ => {
                return Err(D::Error::custom(
                    "a type's bounty gives an amount or a cost",
                ));
            }
        };
        Ok(Self { resource, pays })
    }
}

impl DataSection for ModeBountyData {
    const TABLE: DataTable = DataTable::new("The mode's `[bounty]`");
}

impl Section for ModeBountyData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "bounty";
}

impl DataSection for BountyData {
    const TABLE: DataTable = DataTable::new("A unit type's `bounty`");
}

impl Section for BountyData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "bounty";
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_bounty_pays_an_amount_or_a_share_of_a_cost_and_no_share_below_0() {
        let pays = |text: &str| Toml::parse::<BountyData>(text).map(|data| data.pays);
        assert_eq!(
            pays("resource = \"gold\"\namount = 20").unwrap(),
            Pays::Amount(20)
        );
        assert_eq!(
            pays("resource = \"money\"\ncost = 2000").unwrap(),
            Pays::Cost(2000)
        );
        for refused in [
            "resource = \"gold\"",
            "resource = \"gold\"\namount = 2\ncost = 3",
            "resource = \"gold\"\namount = -1",
        ] {
            assert!(pays(refused).is_err(), "{refused}");
        }
        // As the import writes one, it reads back.
        let written = BountyData {
            resource: DeclaredName::new("money").unwrap(),
            pays: Pays::Cost(900),
        };
        assert_eq!(
            Toml::parse::<BountyData>(&Toml::write(&written)).unwrap(),
            written
        );
        let mode = |text: &str| Toml::parse::<ModeBountyData>(text);
        assert_eq!(mode("killer = \"enemies\"").unwrap().share, Num::ZERO);
        let half = Num::int(1) / Num::int(2);
        assert_eq!(
            mode("killer = \"enemies\"\nshare = \"0.5\"").unwrap().share,
            half
        );
        assert!(mode("killer = \"enemies\"\nshare = \"-0.5\"").is_err());
    }
}
