use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_script::UnknownName;
use campfire_values::{NameId, Named, TrackId, UnitType};

use crate::track_book::TrackBook;
use crate::type_tracks_data::TypeTracksData;

/// The tracks a unit has, of the mode's at most `TrackId::LIMIT`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrackSet(u32);

impl FromIterator<TrackId> for TrackSet {
    fn from_iter<I: IntoIterator<Item = TrackId>>(tracks: I) -> Self {
        let mut set = Self::default();
        for track in tracks {
            set.0 |= 1 << track.index();
        }
        set
    }
}

impl<const N: usize> From<[TrackId; N]> for TrackSet {
    fn from(tracks: [TrackId; N]) -> Self {
        tracks.into_iter().collect()
    }
}

impl TrackSet {
    pub const fn contains(self, track: TrackId) -> bool {
        self.0 & (1 << track.get()) != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Its tracks, in the order of their ids.
    pub fn iter(self) -> impl Iterator<Item = TrackId> {
        (0..TrackId::LIMIT)
            .map(TrackId::at)
            .filter(move |&track| self.contains(track))
    }
}

impl PerId<UnitType> for TrackSet {
    const SCRIPTS_ONLY: bool = false;
}

impl TrackSet {
    /// The tracks of each unit type that stands and gains experience; an error for a track the
    /// mode does not declare.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let names = cx.book::<TrackBook>()?.names();
        let mut book = ByIdBuilder::default();
        for unit in cx.standing() {
            let Some(tracks) = unit.data.section::<TypeTracksData>() else {
                continue;
            };
            let set = tracks.0.iter().map(|track| {
                let named = names.named(track.as_str());
                named.ok_or_else(|| {
                    let at = BookPlace::UnitType(unit.place());
                    BookError::unknown(unit.package, at, UnknownName::new(TrackId::NAMES, track))
                })
            });
            let set: Self = set.collect::<Result<_, BookError>>()?;
            if !tracks.0.is_empty() {
                book.set(unit.unit_type, set);
            }
        }
        Ok(book.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_holds_exactly_the_tracks_it_was_made_of() {
        let [first, fifth, last] = [0, 4, 31].map(TrackId::new);
        let set = TrackSet::from([last, first, fifth, first]);
        assert_eq!(set.iter().collect::<Vec<_>>(), [first, fifth, last]);
        assert!(set.contains(last) && !set.contains(TrackId::new(1)));
        assert!(TrackSet::default().is_empty() && !set.is_empty());
    }
}
