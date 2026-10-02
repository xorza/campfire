use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::progression::track_book::TrackBook;
use crate::progression::track_set::TrackSet;
use crate::stats::level::Level;
use crate::units::track_id::TrackId;

/// A unit's experience on each of its tracks, in the order of their ids, and its level on each but
/// the `level` track, whose level is the unit's `Level`. A level is state, not derived from
/// experience: experience raises it, and never lowers it.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Experience {
    tracks: Vec<TrackXp>,
}

/// A unit's experience on one track, and its level there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackXp {
    pub track: TrackId,
    pub xp: Num,
    /// Its level; none on the `level` track, whose level is the unit's.
    pub level: Option<Level>,
}

/// The levels a unit's track went from and to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Raised {
    pub(crate) from: Level,
    pub(crate) to: Level,
}

impl Experience {
    /// No experience on each of `tracks`, each at level 1 but `level_track`, the mode's `level`
    /// track, whose level is the unit's.
    pub(crate) fn new(tracks: TrackSet, level_track: Option<TrackId>) -> Experience {
        let tracks = tracks.iter().map(|track| TrackXp {
            track,
            xp: Num::ZERO,
            level: (Some(track) != level_track).then(Level::default),
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
    /// higher; on the `level` track, that level is `unit_level`, the unit's.
    pub(crate) fn add(
        &mut self,
        track: TrackId,
        amount: Num,
        book: &TrackBook,
        unit_level: Option<&mut Level>,
    ) -> Raised {
        debug_assert!(amount >= Num::ZERO, "experience added is not negative");
        let held = self
            .tracks
            .iter_mut()
            .find(|held| held.track == track)
            .expect("a unit given experience has the track");
        held.xp = held.xp.checked_add(amount).unwrap_or(Num::MAX);
        let level = match (&mut held.level, unit_level) {
            (Some(own), _) => own,
            (None, Some(unit)) => unit,
            (None, None) => panic!("the level track's level is its unit's"),
        };
        let from = *level;
        *level = from.max(book.level_at(track, held.xp));
        Raised { from, to: *level }
    }
}

impl SimComponent for Experience {
    const NAME: &'static str = "progression.experience";

    // A track the mode lacks has no thresholds to count levels by; the `level` track's level is
    // the unit's, so it holds one of its own exactly on every other track.
    fn check(&self, world: &World, _: Entity) -> bool {
        let Some(book) = world.get_resource::<TrackBook>() else {
            return self.tracks.is_empty();
        };
        self.tracks.iter().all(|track| {
            let own = track.level.is_some();
            book.has(track.track) && own != (book.level_track() == Some(track.track))
        })
    }
}

/// A snapshot is untrusted, so tracks out of order or twice, or negative experience, fail to
/// decode.
impl<'de> Deserialize<'de> for Experience {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Experience, D::Error> {
        #[derive(Debug, Deserialize)]
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
