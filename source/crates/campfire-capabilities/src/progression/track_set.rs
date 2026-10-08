use crate::units::track_id::TrackId;

/// The tracks a unit has, of the mode's at most `TrackId::LIMIT`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrackSet(u32);

impl TrackSet {
    pub fn of(tracks: impl IntoIterator<Item = TrackId>) -> TrackSet {
        let mut set = TrackSet::default();
        for track in tracks {
            set.0 |= 1 << track.index();
        }
        set
    }

    pub const fn contains(self, track: TrackId) -> bool {
        self.0 & (1 << track.index()) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Its tracks, in the order of their ids.
    pub fn iter(self) -> impl Iterator<Item = TrackId> {
        (0..=u8::MAX)
            .take(TrackId::LIMIT)
            .map(TrackId::new)
            .filter(move |&track| self.contains(track))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_holds_exactly_the_tracks_it_was_made_of() {
        let [first, fifth, last] = [0, 4, 31].map(TrackId::new);
        let set = TrackSet::of([last, first, fifth, first]);
        assert_eq!(set.iter().collect::<Vec<_>>(), [first, fifth, last]);
        assert!(set.contains(last) && !set.contains(TrackId::new(1)));
        assert!(TrackSet::default().is_empty() && !set.is_empty());
    }
}
