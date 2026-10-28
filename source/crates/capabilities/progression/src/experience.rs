use core::num::NonZeroU32;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Changed;
use bevy_ecs::system::Query;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_stats::Level;
use campfire_units::{TrackLevel, TrackLevels, UnitEntity};
use campfire_values::TrackId;
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::track_book::TrackBook;
use crate::track_data::TrackThresholds;
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
pub struct TrackRaise {
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

    /// Shows each unit's levels in its `TrackLevels` once its experience changed, as a server's
    /// capability below progression pays by them.
    pub(crate) fn show_levels(mut units: Query<'_, '_, (&Self, &mut TrackLevels), Changed<Self>>) {
        for (experience, mut levels) in &mut units {
            levels.set(experience.iter().map(|held| TrackLevel {
                track: held.track,
                level: NonZeroU32::new(held.level.get()).expect("a level is 1 at least"),
            }));
        }
    }

    /// Adds `amount`, not negative, to `track`, which the unit has, at most up to the largest
    /// number: its level rises to the one the experience reaches by `thresholds`, if that is
    /// higher.
    pub fn add(
        &mut self,
        track: TrackId,
        amount: Num,
        thresholds: TrackThresholds<'_>,
    ) -> TrackRaise {
        debug_assert!(amount >= Num::ZERO, "experience added is not negative");
        let held = self
            .tracks
            .iter_mut()
            .find(|held| held.track == track)
            .expect("a unit given experience has the track");
        held.xp = held.xp.checked_add(amount).unwrap_or(Num::MAX);
        let from = held.level;
        held.level = from.max(thresholds.level_at(held.xp));
        TrackRaise {
            from,
            to: held.level,
        }
    }
}

impl SimComponent for Experience {
    const NAME: &'static str = "progression.experience";
    type Host = UnitEntity;

    // A track the mode lacks has no thresholds to count levels by, a level is one its track has,
    // as a death's experience by level reads its value at it, and the `level` track's level is
    // the unit's.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<TrackBook>();
        let unit_level = world.get::<Level>(entity).copied();
        self.tracks.iter().all(|held| {
            book.names().has(held.track)
                && usize::try_from(held.level.get())
                    .is_ok_and(|level| level <= book.levels(held.track))
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

#[cfg(test)]
mod tests {
    use campfire_codec::{Binary, BinaryError};

    use super::*;

    const fn track(at: u8) -> TrackId {
        TrackId::new(at)
    }

    #[test]
    fn a_level_is_reached_exactly_at_its_threshold_and_never_falls() {
        let book = TrackBook::sample();
        assert_eq!(book.level_track(), Some(track(0)));
        assert_eq!(book.names().named("valor"), Some(track(1)));
        assert_eq!(book.names().name(track(1)), Some("valor"));
        // A threshold met is a level: 99.99… is 1, 100 is 2, 299 is 2, 300 and anything past
        // are 3.
        let level = |xp| book.thresholds(track(0)).level_at(xp).get();
        let below = Num::int(100) - Num::EPSILON;
        let levels = [below, Num::int(100), Num::int(299), Num::int(300), Num::MAX].map(level);
        assert_eq!(levels, [1, 2, 2, 3, 3]);
        assert_eq!(book.thresholds(track(1)).level_at(Num::int(50)).get(), 2);

        // Experience adds up to the largest number at most, and a level set past what it reaches
        // stays: a level is state. Each track holds its own level, the `level` track's too.
        let mut experience = Experience::new(TrackSet::from([track(0), track(1)]));
        let level_of = |experience: &Experience, at| experience.get(track(at)).unwrap().level.get();
        assert_eq!([0, 1].map(|at| level_of(&experience, at)), [1, 1]);
        let raised = experience.add(track(0), Num::int(150), book.thresholds(track(0)));
        assert_eq!((raised.from.get(), raised.to.get()), (1, 2));
        let raised = experience.add(track(0), Num::MAX, book.thresholds(track(0)));
        assert_eq!((raised.from.get(), raised.to.get()), (2, 3));
        assert_eq!(experience.get(track(0)).unwrap().xp, Num::MAX);
        assert_eq!([0, 1].map(|at| level_of(&experience, at)), [3, 1]);
        let mut ahead = Experience::new(TrackSet::from([track(1)]));
        ahead.tracks_mut()[0].level = Level::new(5).unwrap();
        let raised = ahead.add(track(1), Num::int(60), book.thresholds(track(1)));
        assert_eq!((raised.from.get(), raised.to.get()), (5, 5));
        assert_eq!(ahead.get(track(0)), None);
    }

    #[test]
    fn a_snapshot_with_tracks_out_of_order_or_negative_experience_fails_to_decode() {
        #[derive(Debug, Serialize)]
        struct Fields {
            tracks: Vec<TrackXp>,
        }
        let held = |at, xp| TrackXp {
            track: track(at),
            xp,
            level: Level::default(),
        };
        let decode = |tracks: Vec<TrackXp>| {
            let bytes = Binary::encode(&Fields { tracks });
            Binary::decode::<Experience>(&bytes)
        };
        assert!(decode(vec![held(0, Num::int(5)), held(3, Num::ZERO)]).is_ok());
        assert_eq!(
            decode(vec![held(3, Num::int(5)), held(0, Num::int(5))]).err(),
            Some(BinaryError::REFUSED)
        );
        assert_eq!(
            decode(vec![held(1, Num::int(5)), held(1, Num::int(5))]).err(),
            Some(BinaryError::REFUSED)
        );
        assert_eq!(
            decode(vec![held(0, -Num::EPSILON)]).err(),
            Some(BinaryError::REFUSED)
        );
    }
}
