use std::collections::BTreeMap;
use std::iter;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::DeclaredName;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_math::Num;
use campfire_names::MatchNames;
use campfire_stats::Level;
use campfire_values::{NameId, Names, PackageIndex, TrackId};

use crate::error::TrackError;
use crate::mode_tracks_data::ModeTracksData;
use crate::track_data::{TrackData, TrackHolder, TrackThresholds};
use crate::track_set::TrackSet;

/// The tracks a match loaded, by id, each with the experience of its levels. Package data, not
/// state: a restore loads it from the packages, as a new match does. A clone shares the tracks,
/// as the script view reads them.
#[derive(Resource, Debug, Clone)]
#[component(immutable)]
pub struct TrackBook(Arc<Tracks>);

/// The tracks of a `TrackBook`.
#[derive(Debug)]
struct Tracks {
    /// Shared with the script view.
    names: Arc<Names<TrackId>>,
    /// Every track's thresholds, track after track, from `starts[track]` to the next start.
    thresholds: Vec<Num>,
    starts: Vec<u32>,
    /// Who holds each track, by its id.
    holders: Vec<TrackHolder>,
    /// Every track's points for each level from 1, track after track, from `starts[track]` plus
    /// the track's index, one more than its thresholds.
    points: Vec<u32>,
    level: Option<TrackId>,
}

impl TrackBook {
    /// The book of `tracks`; an error for more than `TrackId::LIMIT`, or more than one the
    /// `level` track.
    pub fn new(tracks: &BTreeMap<DeclaredName, TrackData>) -> Result<Self, TrackError> {
        if tracks.len() > TrackId::LIMIT {
            return Err(TrackError::TooMany);
        }
        let mut names = Names::default();
        let mut book = Tracks {
            names: Arc::default(),
            thresholds: Vec::new(),
            starts: Vec::with_capacity(tracks.len() + 1),
            points: Vec::new(),
            holders: Vec::with_capacity(tracks.len()),
            level: None,
        };
        for (name, track) in tracks {
            let id = names.add(name.as_str());
            book.starts.push(Tracks::start(book.thresholds.len()));
            let thresholds = track.levels.get().len();
            book.thresholds.extend_from_slice(track.levels.get());
            if !track.modifiers.is_empty() && track.modifiers.len() != thresholds {
                return Err(TrackError::Modifiers {
                    track: name.clone(),
                    given: track.modifiers.len(),
                    levels: thresholds,
                });
            }
            match &track.points {
                Some(points) if points.len() != thresholds + 1 => {
                    return Err(TrackError::Points {
                        track: name.clone(),
                        given: points.len(),
                        levels: thresholds + 1,
                    });
                }
                Some(points) => book.points.extend_from_slice(points),
                None => book.points.extend(iter::repeat_n(1, thresholds + 1)),
            }
            if track.level && track.holder == TrackHolder::Player {
                return Err(TrackError::PlayerLevel(name.clone()));
            }
            if let Some(points) = &track.points
                && !track.level
                && track.holder == TrackHolder::Unit
                && points.iter().any(|&points| points > 0)
            {
                return Err(TrackError::PointsOffLevel(name.clone()));
            }
            book.holders.push(track.holder);
            if track.level {
                if book.level.is_some() {
                    return Err(TrackError::LevelTracks);
                }
                book.level = Some(id);
            }
        }
        book.starts.push(Tracks::start(book.thresholds.len()));
        book.names = Arc::new(names);
        Ok(Self(Arc::new(book)))
    }

    /// The tracks' names.
    pub fn names(&self) -> &Arc<Names<TrackId>> {
        &self.0.names
    }

    /// The track that is its units' `level`, if one is.
    pub fn level_track(&self) -> Option<TrackId> {
        self.0.level
    }

    /// How many levels `track` has: 1, and one for each threshold.
    pub fn levels(&self, track: TrackId) -> usize {
        let at = track.index();
        (self.0.starts[at + 1] - self.0.starts[at]) as usize + 1
    }

    /// Who holds `track`.
    pub fn holder(&self, track: TrackId) -> TrackHolder {
        self.0.holders[track.index()]
    }

    /// The tracks players hold.
    pub fn player_tracks(&self) -> TrackSet {
        let tracks = (0..self.0.holders.len()).map(TrackId::at);
        tracks
            .filter(|&track| self.holder(track) == TrackHolder::Player)
            .collect()
    }

    /// The points of each of `track`'s levels from 1.
    fn points(&self, track: TrackId) -> &[u32] {
        let at = track.index();
        let start = self.0.starts[at] as usize + at;
        let end = self.0.starts[at + 1] as usize + at + 1;
        &self.0.points[start..end]
    }

    /// The points `track`'s levels give from level 1 through `level`, one it has.
    pub fn points_to(&self, track: TrackId, level: Level) -> u32 {
        let through = usize::try_from(level.get()).expect("a level fits usize");
        self.points(track)[..through]
            .iter()
            .fold(0, |sum, &points| {
                sum.checked_add(points).expect("a track's points fit u32")
            })
    }

    /// Whether any of `track`'s levels gives a point.
    pub fn gives_points(&self, track: TrackId) -> bool {
        self.points(track).iter().any(|&points| points > 0)
    }

    /// The thresholds of `track`'s levels from 2.
    pub fn thresholds(&self, track: TrackId) -> TrackThresholds<'_> {
        let at = track.index();
        let start = self.0.starts[at] as usize;
        let end = self.0.starts[at + 1] as usize;
        TrackThresholds::of(&self.0.thresholds[start..end])
    }
}

impl Tracks {
    fn start(at: usize) -> u32 {
        u32::try_from(at).expect("thresholds fit u32")
    }
}

impl Default for TrackBook {
    fn default() -> Self {
        Self::new(&BTreeMap::new()).expect("no tracks break no rule")
    }
}

impl Book for TrackBook {
    const SCRIPTS_ONLY: bool = false;

    /// Names the tracks to scripts; the progression column takes the book from the match at each
    /// read.
    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(self.names()));
    }
}

impl TrackBook {
    /// The mode's `[tracks]`; an error for more tracks than a match holds, or more than one that
    /// counts the units' level.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let none = BTreeMap::new();
        let tracks = cx.mode_section::<ModeTracksData>();
        let tracks = tracks.map_or(&none, |tracks| &tracks.0);
        Self::new(tracks).map_err(|error| {
            let at = BookPlace::Section(ModeTracksData::KEY);
            BookError::rule(PackageIndex::MODE, at, error)
        })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_common::DeclaredName;
    use campfire_math::Num;

    use crate::track_book::TrackBook;
    use crate::track_data::{Thresholds, TrackData};

    impl TrackBook {
        /// The tests' tracks: `level`, its units' level, of levels 2 at 100 and 3 at 300, and
        /// `valor`, of level 2 at 50.
        pub fn sample() -> Self {
            let data = |levels: &[i64], level| {
                let levels = Thresholds::new(levels.iter().map(|&value| Num::int(value)));
                TrackData::new(levels.unwrap(), level)
            };
            let tracks = [
                ("level", data(&[100, 300], true)),
                ("valor", data(&[50], false)),
            ]
            .map(|(name, data)| (DeclaredName::new(name).unwrap(), data));
            Self::new(&tracks.into()).unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::points::Points;
    use crate::track_data::Thresholds;

    use super::*;

    fn track(levels: &[i64], level: bool) -> TrackData {
        let levels = Thresholds::new(levels.iter().map(|&value| Num::int(value)));
        TrackData::new(levels.unwrap(), level)
    }

    fn book(
        tracks: impl IntoIterator<Item = (&'static str, TrackData)>,
    ) -> Result<TrackBook, TrackError> {
        let tracks = tracks
            .into_iter()
            .map(|(name, data)| (DeclaredName::new(name).unwrap(), data));
        TrackBook::new(&tracks.collect())
    }

    #[test]
    fn a_track_gives_its_levels_points_and_its_modifiers_one_for_each_level() {
        // A point a level by default: levels 1 to 3 give 1, 2 and 3 in all. `points = [0, 2, 1]`
        // gives 0, 2 and 3; all of none gives no point, and a unit of it holds none.
        let id = TrackId::new(0);
        let default = book([("level", track(&[100, 300], true))]).unwrap();
        let to = |book: &TrackBook, level| book.points_to(id, Level::new(level).unwrap());
        assert_eq!([1, 2, 3].map(|level| to(&default, level)), [1, 2, 3]);
        let given = |points: Vec<u32>| TrackData {
            points: Some(points),
            ..track(&[100, 300], true)
        };
        let some = book([("level", given(vec![0, 2, 1]))]).unwrap();
        assert_eq!([1, 2, 3].map(|level| to(&some, level)), [0, 2, 3]);
        assert_eq!(
            Points::at_spawn(TrackSet::from([id]), &some).map(Points::get),
            Some(0)
        );
        let none = book([("level", given(vec![0, 0, 0]))]).unwrap();
        assert!(default.gives_points(id) && !none.gives_points(id));
        assert_eq!(Points::at_spawn(TrackSet::from([id]), &none), None);
        // Points one for each level, modifiers one for each level from 2, and points on the
        // `level` track alone.
        let name = || DeclaredName::new("level").unwrap();
        assert_eq!(
            book([("level", given(vec![1, 1]))]).err(),
            Some(TrackError::Points {
                track: name(),
                given: 2,
                levels: 3
            })
        );
        let modifiers = TrackData {
            modifiers: vec![DeclaredName::new("veteran").unwrap()],
            ..track(&[100, 300], true)
        };
        assert_eq!(
            book([("level", modifiers)]).err(),
            Some(TrackError::Modifiers {
                track: name(),
                given: 1,
                levels: 2
            })
        );
        let off = TrackData {
            points: Some(vec![1, 1]),
            ..track(&[50], false)
        };
        assert_eq!(
            book([("valor", off)]).err(),
            Some(TrackError::PointsOffLevel(
                DeclaredName::new("valor").unwrap()
            ))
        );
    }
}
