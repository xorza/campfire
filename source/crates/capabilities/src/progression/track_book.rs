use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_math::Num;

use crate::declare::book::Book;
use crate::progression::error::TrackError;
use crate::progression::progression_column::ProgressionColumn;
use crate::progression::track_data::TrackData;
use crate::scripts::frame::Frame;
use crate::stats::level::Level;
use crate::units::track_id::TrackId;
use crate::units::view::View;
use crate::values::declared_name::DeclaredName;
use crate::values::names::Names;
use campfire_sim::Capability;

/// The tracks a match loaded, by id, each with the experience of its levels. Package data, not
/// state: a restore loads it from the packages, as a new match does. A clone shares the tracks,
/// as the script view reads them.
#[derive(Resource, Debug, Clone)]
pub(crate) struct TrackBook(Arc<Tracks>);

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
    pub(crate) fn new(tracks: &BTreeMap<DeclaredName, TrackData>) -> Result<TrackBook, TrackError> {
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
        Ok(TrackBook(Arc::new(book)))
    }

    /// The tracks' names.
    pub(crate) fn names(&self) -> &Arc<Names<TrackId>> {
        &self.0.names
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
        TrackBook::new(&BTreeMap::new()).expect("no tracks break no rule")
    }
}

impl Book for TrackBook {
    const OWNER: Option<Capability> = Some(Capability::Progression);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View, _: Option<&mut Frame>) {
        ProgressionColumn::share(view, self.clone());
    }
}
