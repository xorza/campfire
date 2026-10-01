use bevy_ecs::component::Component;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::progression::track_book::TrackBook;
use crate::progression::track_id::TrackId;
use crate::progression::track_set::TrackSet;
use crate::stats::level::Level;

/// A unit's experience and level on each of its tracks, in the order of their ids. A level is
/// state, not derived from experience: experience raises it, and never lowers it.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Experience {
    tracks: Vec<TrackXp>,
}

/// A unit's experience on one track, and its level there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackXp {
    pub track: TrackId,
    pub xp: Num,
    pub level: Level,
}

/// The levels a unit's track went from and to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Raised {
    pub(crate) from: Level,
    pub(crate) to: Level,
}

impl Experience {
    /// No experience on each of `tracks`, each at level 1.
    pub(crate) fn new(tracks: TrackSet) -> Experience {
        let tracks = tracks.iter().map(|track| TrackXp {
            track,
            xp: Num::ZERO,
            level: Level::default(),
        });
        Experience {
            tracks: tracks.collect(),
        }
    }

    pub(crate) fn tracks(&self) -> TrackSet {
        TrackSet::of(self.tracks.iter().map(|track| track.track))
    }

    pub fn get(&self, track: TrackId) -> Option<TrackXp> {
        self.tracks.iter().find(|held| held.track == track).copied()
    }

    /// Adds `amount`, not negative, to `track`, which the unit has, at most up to the largest
    /// number: its level rises to the one the experience reaches on `book`'s track, if that is
    /// higher.
    pub(crate) fn add(&mut self, track: TrackId, amount: Num, book: &TrackBook) -> Raised {
        debug_assert!(amount >= Num::ZERO, "experience added is not negative");
        let held = self
            .tracks
            .iter_mut()
            .find(|held| held.track == track)
            .expect("a unit given experience has the track");
        held.xp = held.xp.checked_add(amount).unwrap_or(Num::MAX);
        let from = held.level;
        held.level = from.max(book.level_at(track, held.xp));
        Raised {
            from,
            to: held.level,
        }
    }
}

impl SimComponent for Experience {
    const NAME: &'static str = "progression.experience";
}

/// A snapshot is untrusted, so tracks out of order or twice, or negative experience, fail to
/// decode.
impl<'de> Deserialize<'de> for Experience {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Experience, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            tracks: Vec<TrackXp>,
        }
        let Fields { tracks } = Fields::deserialize(deserializer)?;
        if !tracks.is_sorted_by(|a, b| a.track < b.track) {
            return Err(D::Error::custom("tracks in order, each once"));
        }
        if tracks.iter().any(|track| track.xp < Num::ZERO) {
            return Err(D::Error::custom("experience is not negative"));
        }
        Ok(Experience { tracks })
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::progression::experience::{Experience, TrackXp};

    impl Experience {
        /// Its tracks, to set a level as a script would.
        pub(crate) fn tracks_mut(&mut self) -> &mut [TrackXp] {
            &mut self.tracks
        }
    }
}
