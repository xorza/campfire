use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use campfire_stats::Level;

use crate::track_data::Thresholds;

/// A unit type's `tracks`: the mode's tracks it gains experience on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TypeTracksData(pub Vec<TypeTrackData>);

/// An entry of a type's `tracks`: a track's name, or `{ track, levels, starts }`, the type's own
/// thresholds on it, of the count of the track's, as Zero Hour's `ExperienceRequired` is each
/// type's (generals.md, A type's own thresholds), and the levels its units start at, each always
/// or while their player holds a modifier, the highest that holds (generals.md, A start level by
/// a modifier).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeTrackData {
    pub track: DeclaredName,
    pub levels: Option<Thresholds>,
    pub starts: Vec<StartData>,
}

/// An entry of `starts`, `{ level, requires }`: the level from 2 a unit spawns at, always, or
/// while its player holds the modifier `requires` of the type's package, as each of Zero Hour's
/// `VeterancyGainCreate` modules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartData {
    pub level: Level,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<DeclaredName>,
}

impl From<DeclaredName> for TypeTrackData {
    fn from(track: DeclaredName) -> Self {
        Self {
            track,
            levels: None,
            starts: Vec::new(),
        }
    }
}

impl<'de> Deserialize<'de> for TypeTrackData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Written {
            Name(DeclaredName),
            Own(Own),
        }
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Own {
            track: DeclaredName,
            levels: Option<Thresholds>,
            #[serde(default)]
            starts: Vec<StartData>,
        }
        Ok(match Written::deserialize(deserializer)? {
            Written::Name(track) => Self::from(track),
            Written::Own(Own {
                track,
                levels,
                starts,
            }) => Self {
                track,
                levels,
                starts,
            },
        })
    }
}

/// As data writes it: a name alone for an entry of nothing of its own, else a table.
impl Serialize for TypeTrackData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.levels.is_none() && self.starts.is_empty() {
            return self.track.serialize(serializer);
        }
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("track", &self.track)?;
        if let Some(levels) = &self.levels {
            map.serialize_entry("levels", levels)?;
        }
        if !self.starts.is_empty() {
            map.serialize_entry("starts", &self.starts)?;
        }
        map.end()
    }
}

impl Section for TypeTracksData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "tracks";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;
    use campfire_math::Num;
    use serde::Deserialize;

    use super::*;

    #[test]
    fn an_entry_is_a_name_or_a_track_of_the_type_s_own_thresholds() {
        #[derive(Debug, Deserialize)]
        struct Holder {
            tracks: TypeTracksData,
        }
        let read = |text: &str| Toml::parse::<Holder>(text).map(|holder| holder.tracks.0);
        let name = |name: &str| DeclaredName::new(name).unwrap();
        let entries =
            read("tracks = [\"level\", { track = \"veterancy\", levels = [200, 300, 600] }]")
                .unwrap();
        assert_eq!(
            entries,
            [
                TypeTrackData::from(name("level")),
                TypeTrackData {
                    track: name("veterancy"),
                    levels: Thresholds::new([200, 300, 600].map(Num::int)),
                    starts: Vec::new(),
                },
            ]
        );
        // A start always, and one while its player holds `training`.
        let starts = read(
            "tracks = [{ track = \"veterancy\", starts = [{ level = 2 }, \
             { level = 3, requires = \"training\" }] }]",
        );
        let start = |level, requires: Option<&str>| StartData {
            level: Level::new(level).unwrap(),
            requires: requires.map(name),
        };
        assert_eq!(
            starts.unwrap()[0].starts,
            [start(2, None), start(3, Some("training"))]
        );
        for refused in [
            "tracks = [{ track = \"veterancy\", levels = [300, 200] }]",
            "tracks = [{ track = \"veterancy\", levels = [200], starts = 2 }]",
            "tracks = [{ track = \"veterancy\", starts = [{ level = 0, requires = \"x\" }] }]",
            "tracks = [{ track = \"veterancy\", start = { level = 2 } }]",
        ] {
            assert!(read(refused).is_err(), "{refused}");
        }
    }
}
