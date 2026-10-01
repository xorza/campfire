use std::collections::BTreeMap;
use std::rc::Rc;

use bevy_ecs::resource::Resource;
use campfire_math::Num;

use crate::progression::track_data::TrackData;
use crate::progression::track_id::TrackId;
use crate::stats::level::Level;
use crate::values::declared_name::DeclaredName;

/// The tracks a match loaded, by id, each with the experience of its levels. Package data, not
/// state: a restore loads it from the packages, as a new match does.
#[derive(Resource, Debug)]
pub(crate) struct TrackBook {
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
        let mut book = TrackBook {
            names: tracks.keys().cloned().collect(),
            thresholds: Vec::new(),
            starts: Vec::with_capacity(tracks.len() + 1),
            level: None,
        };
        for (at, track) in tracks.values().enumerate() {
            book.starts.push(TrackBook::start(book.thresholds.len()));
            book.thresholds.extend_from_slice(track.levels.get());
            if track.level {
                assert!(book.level.is_none(), "the load checked the level tracks");
                book.level = TrackId::new(at);
            }
        }
        book.starts.push(TrackBook::start(book.thresholds.len()));
        book
    }

    fn start(at: usize) -> u32 {
        u32::try_from(at).expect("thresholds fit u32")
    }

    /// Its tracks' names, by id, for the script view.
    pub(crate) fn names(&self) -> Rc<[DeclaredName]> {
        Rc::from(&*self.names)
    }

    /// The track `name`, if the mode declares it.
    pub(crate) fn id(&self, name: &DeclaredName) -> Option<TrackId> {
        let at = self.names.iter().position(|held| held == name)?;
        TrackId::new(at)
    }

    /// The track that is its units' `level`, if one is.
    pub(crate) const fn level_track(&self) -> Option<TrackId> {
        self.level
    }

    /// The level `xp` reaches on `track`: 1, and one more for each threshold it meets.
    pub(crate) fn level_at(&self, track: TrackId, xp: Num) -> Level {
        let at = track.index();
        let start = self.starts[at] as usize;
        let end = self.starts[at + 1] as usize;
        let met = self.thresholds[start..end].partition_point(|&threshold| threshold <= xp);
        Level::new(u32::try_from(met + 1).expect("levels fit u32")).expect("a level from 1")
    }
}

impl Default for TrackBook {
    fn default() -> TrackBook {
        TrackBook::new(&BTreeMap::new())
    }
}
