use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::DeclaredName;
use campfire_declarations::Book;
use campfire_math::Num;
use campfire_script::MatchNames;
use campfire_stats::Level;
use campfire_values::{NameId, Names, TrackId};

use crate::error::TrackError;
use crate::track_data::TrackData;

/// The tracks a match loaded, by id, each with the experience of its levels. Package data, not
/// state: a restore loads it from the packages, as a new match does. A clone shares the tracks,
/// as the script view reads them.
#[derive(Resource, Debug, Clone)]
pub struct TrackBook(Arc<Tracks>);

/// The tracks of a `TrackBook`.
#[derive(Debug)]
struct Tracks {
    /// Shared with the script view.
    names: Arc<Names<TrackId>>,
    /// Every track's thresholds, track after track, from `starts[track]` to the next start.
    thresholds: Vec<Num>,
    starts: Vec<u32>,
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
            level: None,
        };
        for (name, track) in tracks {
            let id = names.add(name.as_str());
            book.starts.push(Tracks::start(book.thresholds.len()));
            book.thresholds.extend_from_slice(track.levels.get());
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

    /// The level `xp` reaches on `track`: 1, and one more for each threshold it meets.
    pub fn level_at(&self, track: TrackId, xp: Num) -> Level {
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
