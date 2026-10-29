use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_names::UnknownName;
use campfire_stats::{Level, ModifierBook};
use campfire_values::{ModifierId, Named, PackageIndex, TrackId, UnitType};

use crate::error::TrackError;
use crate::track_book::TrackBook;
use crate::track_data::{Thresholds, TrackHolder, TrackThresholds};
use crate::track_set::TrackSet;
use crate::type_tracks_data::TypeTracksData;

/// A unit type's tracks, and its own thresholds on those it gives them for
/// (04-capabilities/progression.md, A type's own thresholds).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeTracks {
    set: TrackSet,
    own: Box<[OwnThresholds]>,
    starts: Box<[TrackStart]>,
}

/// Why a type's tracks do not resolve: a name the match does not have, or a rule of the tracks.
#[derive(Debug)]
pub(crate) enum TracksRefusal {
    Unknown(UnknownName),
    Rule(TrackError),
}

/// A level a type's units start at on one of its tracks, always or while their player holds a
/// modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TrackStart {
    pub(crate) track: TrackId,
    pub(crate) level: Level,
    pub(crate) requires: Option<ModifierId>,
}

/// A type's own thresholds on one of its tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnThresholds {
    track: TrackId,
    levels: Thresholds,
}

impl From<TrackSet> for TypeTracks {
    fn from(set: TrackSet) -> Self {
        Self {
            set,
            own: Box::default(),
            starts: Box::default(),
        }
    }
}

impl TypeTracks {
    pub const fn set(&self) -> TrackSet {
        self.set
    }

    /// The levels its units start at, always or while their player holds a modifier.
    pub(crate) fn starts(&self) -> &[TrackStart] {
        &self.starts
    }

    /// The thresholds its units count their levels on `track` by: its own, or the track's of
    /// `book`.
    pub fn thresholds<'a>(&'a self, track: TrackId, book: &'a TrackBook) -> TrackThresholds<'a> {
        self.own
            .iter()
            .find(|own| own.track == track)
            .map_or_else(|| book.thresholds(track), |own| own.levels.view())
    }
}

impl PerId<UnitType> for TypeTracks {
    const SCRIPTS_ONLY: bool = false;
}

impl TypeTracks {
    /// The tracks of each unit type that stands and gains experience; an error for a track the
    /// mode does not declare, for own thresholds of another count than the track's, and for a
    /// start at a level past the track's or of a modifier the package does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let tracks = cx.book::<TrackBook>()?;
        let modifiers = cx.book::<ModifierBook>()?;
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<TypeTracksData>() else {
                continue;
            };
            if data.0.is_empty() {
                continue;
            }
            let at = || BookPlace::UnitType(unit.place());
            let resolved = Self::resolve(data, tracks, modifiers, unit.package);
            let tracks = resolved.map_err(|refusal| match refusal {
                TracksRefusal::Unknown(unknown) => BookError::unknown(unit.package, at(), unknown),
                TracksRefusal::Rule(error) => BookError::rule(unit.package, at(), error),
            })?;
            book.set(unit.unit_type, tracks);
        }
        Ok(book.build())
    }

    /// The tracks `data` gives a type of `package`, by `tracks` and `modifiers`.
    pub(crate) fn resolve(
        data: &TypeTracksData,
        tracks: &TrackBook,
        modifiers: &ModifierBook,
        package: PackageIndex,
    ) -> Result<Self, TracksRefusal> {
        let mut set = Vec::with_capacity(data.0.len());
        let mut own = Vec::new();
        let mut starts = Vec::new();
        for entry in &data.0 {
            let track = tracks.names().named(entry.track.as_str()).ok_or_else(|| {
                TracksRefusal::Unknown(UnknownName::new(TrackId::NAMES, &entry.track))
            })?;
            if tracks.holder(track) == TrackHolder::Player {
                return Err(TracksRefusal::Rule(TrackError::PlayerTrack(
                    entry.track.clone(),
                )));
            }
            set.push(track);
            for start in &entry.starts {
                let levels = tracks.levels(track);
                let level = usize::try_from(start.level.get()).expect("a level fits usize");
                if level < 2 || level > levels {
                    return Err(TracksRefusal::Rule(TrackError::StartLevel {
                        track: entry.track.clone(),
                        level: start.level.get(),
                        levels,
                    }));
                }
                let requires = start
                    .requires
                    .as_ref()
                    .map(|requires| {
                        let named = modifiers.names().named((package, requires.as_str()));
                        named.ok_or_else(|| {
                            TracksRefusal::Unknown(UnknownName::new(ModifierId::NAMES, requires))
                        })
                    })
                    .transpose()?;
                starts.push(TrackStart {
                    track,
                    level: start.level,
                    requires,
                });
            }
            let Some(levels) = &entry.levels else {
                continue;
            };
            let expected = tracks.levels(track) - 1;
            if levels.get().len() != expected {
                return Err(TracksRefusal::Rule(TrackError::TypeLevels {
                    track: entry.track.clone(),
                    given: levels.get().len(),
                    levels: expected,
                }));
            }
            own.push(OwnThresholds {
                track,
                levels: levels.clone(),
            });
        }
        own.sort_unstable_by_key(|own| own.track);
        Ok(Self {
            set: set.into_iter().collect(),
            own: own.into(),
            starts: starts.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_type_counts_its_levels_by_its_own_thresholds_and_else_by_the_track_s() {
        // The sample's `level` reaches level 2 at 100 and 3 at 300; a type of its own 200 and 600
        // reaches 2 at 200 and 3 at 600, and `valor`, of none of its own, at the track's 50.
        let book = TrackBook::sample();
        let [level, valor] = [0, 1].map(TrackId::new);
        let own = TypeTracks {
            set: TrackSet::from([level, valor]),
            own: Box::new([OwnThresholds {
                track: level,
                levels: Thresholds::new([200, 600].map(Num::int)).unwrap(),
            }]),
            starts: Box::default(),
        };
        let shared = TypeTracks::from(TrackSet::from([level, valor]));
        let at = |tracks: &TypeTracks, track, xp: i64| {
            tracks.thresholds(track, &book).level_at(Num::int(xp)).get()
        };
        assert_eq!(
            [150, 200, 599, 600].map(|xp| at(&own, level, xp)),
            [1, 2, 2, 3]
        );
        assert_eq!(
            [150, 200, 599, 600].map(|xp| at(&shared, level, xp)),
            [2, 2, 3, 3]
        );
        assert_eq!([49, 50].map(|xp| at(&own, valor, xp)), [1, 2]);
    }
}
