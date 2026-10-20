use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use campfire_values::{FilterData, TagFilterData};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// The mode's `experience` section: the track a death's experience goes to, to whom, and the
/// victims whose deaths give it, by their tags at death, all by default (04-capabilities/games.md,
/// A death's experience).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeExperienceData {
    pub track: DeclaredName,
    pub to: ExperienceTo,
    #[serde(default)]
    pub victim: TagFilterData,
}

/// To whom a death's experience goes: its killer, `"killer"`, or `{ radius, filter }`, the living
/// units `filter` selects for the victim whose bodies come within `radius` of its point, split
/// evenly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExperienceTo {
    Killer,
    Near { radius: Num, filter: FilterData },
}

/// A unit type's `experience` section: what its death gives.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperienceData {
    pub gives: Gives,
}

/// What a death gives, never below 0: a number; a list, one for each level of the victim on the
/// mode's track, its first for level 1; or `{ base, per_level }`, `base` and `per_level` for each
/// level the victim holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gives {
    Value(Num),
    ByLevel(Vec<Num>),
    Rising { base: Num, per_level: Num },
}

/// Data is untrusted, so a radius below 0, and a name but `killer`, fail to read.
impl<'de> Deserialize<'de> for ExperienceTo {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Name(String),
            Near(Near),
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Near {
            radius: Num,
            filter: FilterData,
        }
        match Written::deserialize(deserializer)? {
            Written::Name(name) if name == "killer" => Ok(Self::Killer),
            Written::Name(name) => Err(D::Error::custom(format_args!(
                "experience goes to \"killer\" or {{ radius, filter }}, not {name:?}"
            ))),
            Written::Near(Near { radius, filter }) if radius >= Num::ZERO => {
                Ok(Self::Near { radius, filter })
            }
            Written::Near(_) => Err(D::Error::custom("an experience radius is 0 or more")),
        }
    }
}

/// Data is untrusted, so a value below 0, and an empty list, fail to read.
impl<'de> Deserialize<'de> for Gives {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Value(Num),
            ByLevel(Vec<Num>),
            Rising(Rising),
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Rising {
            base: Num,
            per_level: Num,
        }
        let gives = match Written::deserialize(deserializer)? {
            Written::Value(value) => Self::Value(value),
            Written::ByLevel(values) if !values.is_empty() => Self::ByLevel(values),
            Written::ByLevel(_) => {
                return Err(D::Error::custom("a list of experience is not empty"));
            }
            Written::Rising(Rising { base, per_level }) => Self::Rising { base, per_level },
        };
        let values: &[Num] = match &gives {
            Self::Value(value) => &[*value],
            Self::ByLevel(values) => values,
            Self::Rising { base, per_level } => &[*base, *per_level],
        };
        if values.iter().any(|&value| value < Num::ZERO) {
            return Err(D::Error::custom("experience is 0 or more"));
        }
        Ok(gives)
    }
}

impl DataSection for ModeExperienceData {
    const TABLE: DataTable = DataTable::new("The mode's `[experience]`");
}

impl Section for ModeExperienceData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "experience";
}

impl DataSection for ExperienceData {
    const TABLE: DataTable = DataTable::new("A unit type's `experience`");
}

impl Section for ExperienceData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "experience";
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_death_gives_a_number_a_list_by_level_or_a_rise_to_its_killer_or_those_near() {
        let gives = |text: &str| Toml::parse::<ExperienceData>(text).map(|data| data.gives);
        assert_eq!(gives("gives = 59").unwrap(), Gives::Value(Num::int(59)));
        let ranks = [20, 40, 60, 80].map(Num::int).to_vec();
        assert_eq!(
            gives("gives = [20, 40, 60, 80]").unwrap(),
            Gives::ByLevel(ranks)
        );
        assert_eq!(
            gives("gives = { base = 150, per_level = 25 }").unwrap(),
            Gives::Rising {
                base: Num::int(150),
                per_level: Num::int(25)
            }
        );
        for refused in [
            "gives = -1",
            "gives = []",
            "gives = [1, -1]",
            "gives = { base = 1 }",
        ] {
            assert!(gives(refused).is_err(), "{refused}");
        }
        let to = |text: &str| Toml::parse::<ModeExperienceData>(text).map(|data| data.to);
        assert_eq!(
            to("track = \"level\"\nto = \"killer\"").unwrap(),
            ExperienceTo::Killer
        );
        assert_eq!(
            to("track = \"level\"\nto = { radius = \"16.0\", filter = \"enemies:avatar\" }")
                .unwrap(),
            ExperienceTo::Near {
                radius: Num::int(16),
                filter: FilterData::parse("enemies:avatar").unwrap()
            }
        );
        assert!(to("track = \"level\"\nto = \"nearest\"").is_err());
        assert!(to("track = \"level\"\nto = { radius = \"-1\", filter = \"enemies\" }").is_err());
    }
}
