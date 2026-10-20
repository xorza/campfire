use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_state_types::{Replication, SentOnChange};
use campfire_stats::Level;
use campfire_units::UnitEntity;
use campfire_values::TrackId;
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::track_book::TrackBook;
use crate::track_set::TrackSet;

/// A unit's experience and level on each of its tracks, in the order of their ids. A level is
/// state, not derived from experience: experience raises it, and never lowers it. The unit's
/// own level, which stats owns, is its `level` track's.
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
pub struct Raised {
    pub from: Level,
    pub to: Level,
}

impl Experience {
    /// No experience on each of `tracks`, each at level 1.
    pub fn new(tracks: TrackSet) -> Self {
        let tracks = tracks.iter().map(|track| TrackXp {
            track,
            xp: Num::ZERO,
            level: Level::default(),
        });
        Self {
            tracks: tracks.collect(),
        }
    }

    /// Its tracks, in the order of their ids.
    pub fn iter(&self) -> impl Iterator<Item = TrackXp> {
        self.tracks.iter().copied()
    }

    pub fn get(&self, track: TrackId) -> Option<TrackXp> {
        self.tracks.iter().find(|held| held.track == track).copied()
    }

    /// Adds `amount`, not negative, to `track`, which the unit has, at most up to the largest
    /// number: its level rises to the one the experience reaches on `book`'s track, if that is
    /// higher.
    pub fn add(&mut self, track: TrackId, amount: Num, book: &TrackBook) -> Raised {
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
    type Host = UnitEntity;

    // A track the mode lacks has no thresholds to count levels by, and the `level` track's level
    // is the unit's.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<TrackBook>();
        let unit_level = world.get::<Level>(entity).copied();
        self.tracks.iter().all(|held| {
            book.names().has(held.track)
                && (book.level_track() != Some(held.track) || unit_level == Some(held.level))
        })
    }
}

impl Replication for Experience {
    const KIND: DataKind = DataKind::Progression;
    type Sending = SentOnChange;
}

/// A snapshot is untrusted, so tracks out of order or twice, or negative experience, fail to
/// decode.
impl<'de> Deserialize<'de> for Experience {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
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
        Ok(Self { tracks })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::experience::{Experience, TrackXp};

    impl Experience {
        /// Its tracks, to set a level past what its experience reaches.
        pub fn tracks_mut(&mut self) -> &mut [TrackXp] {
            &mut self.tracks
        }
    }
}
