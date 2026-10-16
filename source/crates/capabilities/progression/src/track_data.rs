use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A track as the mode's `[tracks.<name>]` declares it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackData {
    pub levels: Thresholds,
    /// Whether the track is its unit's `level`, which its stats read.
    #[serde(default)]
    pub level: bool,
}

/// The experience a track needs for each level from 2: positive and strictly ascending, so each
/// level needs more than the one before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thresholds(Box<[Num]>);

impl Thresholds {
    /// The thresholds of `levels`; `None` for none, or for one not positive or not more than the
    /// one before.
    pub fn new(levels: impl IntoIterator<Item = Num>) -> Option<Self> {
        let levels: Box<[Num]> = levels.into_iter().collect();
        let ascending = levels.is_sorted_by(|a, b| a < b);
        let positive = levels.first().is_some_and(|&first| first > Num::ZERO);
        (ascending && positive).then_some(Self(levels))
    }

    pub fn get(&self) -> &[Num] {
        &self.0
    }
}

/// Data is untrusted, so thresholds that are none, not positive or not ascending fail to read.
impl<'de> Deserialize<'de> for Thresholds {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Vec::<Num>::deserialize(deserializer)?).ok_or_else(|| {
            D::Error::custom(
                "a track has a level 2, and each level's experience is positive and more than the \
                 level before's",
            )
        })
    }
}

impl DataSection for TrackData {
    const TABLE: DataTable = DataTable::Track;
}
