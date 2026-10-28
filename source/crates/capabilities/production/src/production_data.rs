use std::num::NonZeroU8;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::doors_data::DoorsData;
use crate::frame_point::FramePoint;

/// A unit type's `production` section: the most entries its train queue holds; where its trained
/// units come out, and where they go with no rally point set, each in its own frame
/// (04-capabilities/production.md, A producer may name its exit and rally); whether each gatherer
/// that leaves its exit starts to gather (04-capabilities/production.md, A producer may start its
/// gatherers); the unit types of its package that leave its exit once, as its site completes
/// (04-capabilities/production.md, A building may come with its units); and its places, where each
/// unit it trains is made and which it holds while it lives (04-capabilities/production.md, A jet
/// holds a place); and its doors, which open before each unit leaves (defenses-and-airfields.md, A
/// producer's door opens before its unit leaves).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionData {
    pub queue: NonZeroU8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<FramePoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rally: Option<FramePoint>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gathers: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub free: Vec<DeclaredName>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "PlaceData::read_places"
    )]
    pub places: Vec<PlaceData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doors: Option<DoorsData>,
}

/// A producer's place: its point in the producer's frame, the angle in degrees its unit faces
/// there, turned with the producer, and the point its unit is made at, from which an airfield
/// taxis it to its place, none to make it at its place (defenses-and-airfields.md, A hangar is a
/// jet's start).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceData {
    pub at: FramePoint,
    #[serde(default)]
    pub angle: Num,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<FramePoint>,
}

impl PlaceData {
    /// The most places a producer names, as a `u8` counts each.
    pub const MOST: usize = u8::MAX as usize + 1;

    /// A producer's places, at most `MOST`. Data is untrusted, so more fail to read.
    fn read_places<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Self>, D::Error> {
        let places = Vec::<Self>::deserialize(deserializer)?;
        if places.len() > Self::MOST {
            return Err(D::Error::custom(format!(
                "a producer names at most {} places, not {}",
                Self::MOST,
                places.len()
            )));
        }
        Ok(places)
    }
}

impl DataSection for ProductionData {
    const TABLE: DataTable = DataTable::new("A unit type's `production`");
}

impl Section for ProductionData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "production";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_producer_names_at_most_256_places() {
        let read = |count: usize| {
            let place = r#"{ at = [1, 2], angle = "90" }"#;
            let places = vec![place; count].join(", ");
            Toml::parse::<ProductionData>(&format!("queue = 1\nplaces = [{places}]"))
        };
        let most = read(256).unwrap();
        assert_eq!(most.places.len(), 256);
        assert_eq!(
            most.places[255],
            PlaceData {
                at: FramePoint::new([Num::ONE, Num::int(2)]).unwrap(),
                angle: Num::int(90),
                exit: None,
            }
        );
        assert_eq!(
            read(257).unwrap_err().message(),
            "a producer names at most 256 places, not 257"
        );
    }
}
