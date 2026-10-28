use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use campfire_values::{FilterData, RelationSet, TagFilterData};
use serde::de::Error;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The mode's `[[experience]]`: the shares of a death's experience, each its own track, to
/// whom, and the victims whose deaths give it (generals.md, A death's experience goes by shares).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeExperienceData(pub Vec<ShareData>);

/// A share of a death's experience: the track it goes to, to whom, and the victims whose deaths
/// give it, by their tags at death, all by default (04-capabilities/games.md, A death's
/// experience).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareData {
    pub track: DeclaredName,
    pub to: ExperienceTo,
    pub victim: TagFilterData,
}

/// To whom a death's experience goes: its killer, `"killer"`, or its killer's player, `"player"`,
/// when the killer passes `killer`, a filter from the victim's side, `enemies` by default
/// (generals.md, A share's killer passes a filter; generals.md, A track of players); or `{ radius, filter }`, the living units `filter` selects for the victim whose
/// bodies come within `radius` of its point, split evenly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExperienceTo {
    Killer { killer: FilterData },
    Player { killer: FilterData },
    Near { radius: Num, filter: FilterData },
}

/// As data writes it: its track, to whom, its victims of a filter that leaves any out, and the
/// killers of a share to its killer.
impl Serialize for ShareData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Debug, Serialize)]
        struct Near<'a> {
            radius: Num,
            filter: &'a FilterData,
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("track", &self.track)?;
        match &self.to {
            ExperienceTo::Killer { killer } => {
                map.serialize_entry("to", "killer")?;
                map.serialize_entry("killer", killer)?;
            }
            ExperienceTo::Player { killer } => {
                map.serialize_entry("to", "player")?;
                map.serialize_entry("killer", killer)?;
            }
            ExperienceTo::Near { radius, filter } => {
                map.serialize_entry(
                    "to",
                    &Near {
                        radius: *radius,
                        filter,
                    },
                )?;
            }
        }
        if self.victim != TagFilterData::default() {
            map.serialize_entry("victim", &self.victim)?;
        }
        map.end()
    }
}

/// Data is untrusted, so a radius below 0, a name but `killer`, and a `killer` of a share that
/// goes to the units near fail to read.
impl<'de> Deserialize<'de> for ShareData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            track: DeclaredName,
            to: Written,
            #[serde(default)]
            victim: TagFilterData,
            killer: Option<FilterData>,
        }
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
        let Fields {
            track,
            to,
            victim,
            killer,
        } = Fields::deserialize(deserializer)?;
        let enemies = || FilterData {
            relations: RelationSet::Enemies,
            tags: Vec::new(),
        };
        let to = match (to, killer) {
            (Written::Name(name), killer) if name == "killer" => ExperienceTo::Killer {
                killer: killer.unwrap_or_else(enemies),
            },
            (Written::Name(name), killer) if name == "player" => ExperienceTo::Player {
                killer: killer.unwrap_or_else(enemies),
            },
            (Written::Name(name), _) => {
                return Err(D::Error::custom(format_args!(
                    "experience goes to \"killer\", \"player\" or {{ radius, filter }}, not {name:?}"
                )));
            }
            (Written::Near(_), Some(_)) => {
                return Err(D::Error::custom(
                    "a share's `killer` is of a share to its killer",
                ));
            }
            (Written::Near(Near { radius, filter }), None) if radius >= Num::ZERO => {
                ExperienceTo::Near { radius, filter }
            }
            (Written::Near(_), None) => {
                return Err(D::Error::custom("an experience radius is 0 or more"));
            }
        };
        Ok(Self { track, to, victim })
    }
}

/// A unit type's `experience` section: what its death gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperienceData {
    pub gives: Gives,
}

/// What a death gives, never below 0: a number; a list, one for each level of the victim, its
/// first for level 1; or `{ base, per_level }`, `base` and `per_level` for each level the victim
/// holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gives {
    Value(Num),
    ByLevel(Vec<Num>),
    Rising { base: Num, per_level: Num },
}

/// As data writes it: a number, a list, or `{ base, per_level }`.
impl Serialize for Gives {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Debug, Serialize)]
        struct Rising {
            base: Num,
            per_level: Num,
        }
        match self {
            Self::Value(value) => value.serialize(serializer),
            Self::ByLevel(values) => values.serialize(serializer),
            &Self::Rising { base, per_level } => Rising { base, per_level }.serialize(serializer),
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

impl DataSection for ShareData {
    const TABLE: DataTable = DataTable::new("A share of the mode's `[[experience]]`");
}

impl Section for ModeExperienceData {
    const TABLE: SectionTable = SectionTable::Mode;
    const KEY: &'static str = "experience";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

impl DataSection for ExperienceData {
    const TABLE: DataTable = DataTable::new("A unit type's `experience`");
}

impl Section for ExperienceData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "experience";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_death_gives_a_number_a_list_by_level_or_a_rise_to_its_killer_or_those_near() {
        #[derive(Debug, Deserialize)]
        struct Mode {
            experience: ModeExperienceData,
        }
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
        let to = |text: &str| Toml::parse::<ShareData>(text).map(|data| data.to);
        let filter = |text| FilterData::parse(text).unwrap();
        assert_eq!(
            to("track = \"level\"\nto = \"killer\"").unwrap(),
            ExperienceTo::Killer {
                killer: filter("enemies")
            }
        );
        assert_eq!(
            to("track = \"level\"\nto = \"killer\"\nkiller = \"hostiles\"").unwrap(),
            ExperienceTo::Killer {
                killer: filter("hostiles")
            }
        );
        assert_eq!(
            to("track = \"level\"\nto = { radius = \"16.0\", filter = \"enemies:avatar\" }")
                .unwrap(),
            ExperienceTo::Near {
                radius: Num::int(16),
                filter: filter("enemies:avatar")
            }
        );
        let refusal = |text: &str| to(text).unwrap_err().message().to_owned();
        assert_eq!(
            refusal("track = \"level\"\nto = \"nearest\""),
            "experience goes to \"killer\", \"player\" or { radius, filter }, not \"nearest\""
        );
        assert_eq!(
            refusal("track = \"level\"\nto = { radius = \"-1\", filter = \"enemies\" }"),
            "an experience radius is 0 or more"
        );
        assert_eq!(
            refusal(
                "track = \"level\"\nto = { radius = \"1\", filter = \"enemies\" }\nkiller = \"all\""
            ),
            "a share's `killer` is of a share to its killer"
        );
        let mode = Toml::parse::<Mode>(
            "[[experience]]\ntrack = \"veterancy\"\nto = \"killer\"\n\n[[experience]]\ntrack = \"rank\"\nto = \"killer\"",
        );
        let tracks: Vec<_> = mode
            .unwrap()
            .experience
            .0
            .into_iter()
            .map(|share| share.track)
            .collect();
        assert_eq!(
            tracks,
            ["veterancy", "rank"].map(|name| DeclaredName::new(name).unwrap())
        );
    }
}
