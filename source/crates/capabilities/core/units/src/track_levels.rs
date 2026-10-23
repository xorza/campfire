use core::num::NonZeroU32;

use bevy_ecs::component::Component;
use campfire_values::TrackId;

/// A unit's level on each of its tracks, by track, which progression writes as the unit's
/// experience changes: what a capability below progression pays or scales by. Derived, never
/// state, and a server's alone, as only a server pays by it.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackLevels(Vec<TrackLevel>);

/// A unit's level on one track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackLevel {
    pub track: TrackId,
    pub level: NonZeroU32,
}

impl TrackLevels {
    /// The unit's level on `track`: 1 on a track it does not hold, as a unit without one starts
    /// at level 1 on each.
    pub fn level(&self, track: TrackId) -> NonZeroU32 {
        self.0
            .iter()
            .find(|held| held.track == track)
            .map_or(NonZeroU32::MIN, |held| held.level)
    }

    /// Takes `levels` as the unit's levels, in the order of their tracks.
    pub fn set(&mut self, levels: impl IntoIterator<Item = TrackLevel>) {
        self.0.clear();
        self.0.extend(levels);
        debug_assert!(
            self.0.is_sorted_by_key(|held| held.track),
            "the levels go in the order of their tracks"
        );
    }
}

#[cfg(test)]
mod tests {
    use campfire_values::NameId;

    use super::*;

    /// A held track gives its level; another gives 1; a new set replaces the old.
    #[test]
    fn a_unit_is_at_level_one_on_a_track_it_does_not_hold() {
        let (first, second) = (TrackId::at(0), TrackId::at(1));
        let three = NonZeroU32::new(3).unwrap();
        let mut levels = TrackLevels::default();
        levels.set([TrackLevel {
            track: second,
            level: three,
        }]);
        assert_eq!(levels.level(second), three);
        assert_eq!(levels.level(first), NonZeroU32::MIN);
        levels.set([]);
        assert_eq!(levels.level(second), NonZeroU32::MIN);
    }
}
