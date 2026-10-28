use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use campfire_stats::Level;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A track as the mode's `[tracks.<name>]` declares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackData {
    pub levels: Thresholds,
    /// Whether the track is its unit's `level`, which its stats read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub level: bool,
    /// Who gains experience on it: units, or players (generals.md, A track of players).
    #[serde(default, skip_serializing_if = "TrackHolder::is_unit")]
    pub holder: TrackHolder,
    /// The modifier of the package its units hold at each level from 2, one for each threshold;
    /// none for a track of no modifiers (generals.md, A level holds a modifier).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifiers: Vec<DeclaredName>,
    /// The points each level from 1 gives, one for each level; one each when none
    /// (generals.md, A track gives points by level).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<u32>>,
}

impl TrackData {
    /// A track of `levels`, of no modifiers and a point a level, its units' `level` or not.
    pub const fn new(levels: Thresholds, level: bool) -> Self {
        Self {
            levels,
            level,
            holder: TrackHolder::Unit,
            modifiers: Vec::new(),
            points: None,
        }
    }
}

/// Who gains experience on a track: units of the types that list it, or each player.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackHolder {
    #[default]
    Unit,
    Player,
}

impl TrackHolder {
    /// Whether units hold it, which data leaves out.
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `skip_serializing_if` passes the field by reference"
    )]
    fn is_unit(holder: &Self) -> bool {
        *holder == Self::Unit
    }
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

    pub fn view(&self) -> TrackThresholds<'_> {
        TrackThresholds(&self.0)
    }
}

/// The thresholds a unit's levels on a track are counted by, from level 2: its type's own, or
/// the track's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackThresholds<'a>(&'a [Num]);

impl<'a> TrackThresholds<'a> {
    /// Thresholds of a book that holds them ascending, as `Thresholds::new` checks them.
    pub(crate) fn of(thresholds: &'a [Num]) -> Self {
        debug_assert!(thresholds.is_sorted_by(|a, b| a < b), "thresholds ascend");
        Self(thresholds)
    }

    /// The experience `level`, from 2, needs; none for one past the thresholds.
    pub fn of_level(self, level: Level) -> Option<Num> {
        let at = usize::try_from(level.get()).expect("a level fits usize");
        at.checked_sub(2).and_then(|at| self.0.get(at)).copied()
    }

    /// The level `xp` reaches: 1, and one more for each threshold it meets.
    pub fn level_at(self, xp: Num) -> Level {
        let met = self.0.partition_point(|&threshold| threshold <= xp);
        Level::new(u32::try_from(met + 1).expect("levels fit u32")).expect("a level from 1")
    }
}

/// As data writes it: its thresholds in order.
impl Serialize for Thresholds {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
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
    const TABLE: DataTable = DataTable::new("A track, `[tracks.<name>]`");
}

#[cfg(test)]
mod tests;
