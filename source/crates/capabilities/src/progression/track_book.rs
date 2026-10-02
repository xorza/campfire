use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_math::Num;

use crate::progression::track_data::TrackData;
use crate::progression::track_id::TrackId;
use crate::stats::level::Level;
use crate::values::declared_name::DeclaredName;

/// The tracks a match loaded, by id, each with the experience of its levels. Package data, not
/// state: a restore loads it from the packages, as a new match does. A clone shares the tracks,
/// as the script view reads them.
#[derive(Resource, Debug, Clone)]
pub(crate) struct TrackBook(Arc<Tracks>);

/// The tracks of a `TrackBook`.
#[derive(Debug)]
struct Tracks {
    names: Box<[DeclaredName]>,
    /// Every track's thresholds, track after track, from `starts[track]` to the next start.
    thresholds: Vec<Num>,
    starts: Vec<u32>,
    level: Option<TrackId>,
}

impl TrackBook {
    /// The book of `tracks`, which the package load checked: at most `TrackId::LIMIT`, and at
    /// most one the `level` track.
    pub(crate) fn new(tracks: &BTreeMap<DeclaredName, TrackData>) -> TrackBook {
        assert!(
            tracks.len() <= TrackId::LIMIT,
            "the load checked the tracks"
        );
        let mut book = Tracks {
            names: tracks.keys().cloned().collect(),
            thresholds: Vec::new(),
            starts: Vec::with_capacity(tracks.len() + 1),
            level: None,
        };
        for (at, track) in tracks.values().enumerate() {
            book.starts.push(Tracks::start(book.thresholds.len()));
            book.thresholds.extend_from_slice(track.levels.get());
            if track.level {
                assert!(book.level.is_none(), "the load checked the level tracks");
                book.level = TrackId::new(at);
            }
        }
        book.starts.push(Tracks::start(book.thresholds.len()));
        TrackBook(Arc::new(book))
    }

    /// The track `name`, if the mode declares it.
    pub(crate) fn named(&self, name: &str) -> Option<TrackId> {
        let at = self.0.names.iter().position(|held| held.as_str() == name)?;
        TrackId::new(at)
    }

    /// Every track's name, by id.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.0.names.iter().map(DeclaredName::as_str)
    }

    /// The track that is its units' `level`, if one is.
    pub(crate) fn level_track(&self) -> Option<TrackId> {
        self.0.level
    }

    /// The level `xp` reaches on `track`: 1, and one more for each threshold it meets.
    pub(crate) fn level_at(&self, track: TrackId, xp: Num) -> Level {
        let at = track.index();
        let start = self.0.starts[at] as usize;
        let end = self.0.starts[at + 1] as usize;
        let met = self.0.thresholds[start..end].partition_point(|&threshold| threshold <= xp);
        Level::new(u32::try_from(met + 1).expect("levels fit u32")).expect("a level from 1")
    }
}

impl Tracks {
    fn start(at: usize) -> u32 {
        u32::try_from(at).expect("thresholds fit u32")
    }
}

impl Default for TrackBook {
    fn default() -> TrackBook {
        TrackBook::new(&BTreeMap::new())
    }
}
