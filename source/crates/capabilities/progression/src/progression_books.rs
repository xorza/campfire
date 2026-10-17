use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_actions::{SlotKinds, TrackNames};
use campfire_declare::{BookCx, BookError, BookPlace, ByType, Section};
use campfire_script::{NameKind, UnknownName};
use campfire_values::PackageIndex;

use crate::Progression;
use crate::error::TrackError;
use crate::mode_tracks_data::ModeTracksData;
use crate::track_book::TrackBook;
use crate::track_set::TrackSet;
use crate::type_tracks_data::TypeTracksData;

impl Progression {
    /// The rule of the mode's slot kinds: none gives its ranks levels in a mode with no `level`
    /// track to read them on.
    pub(crate) fn levels_rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        let tracks = cx.mode_section::<ModeTracksData>();
        let level_track = tracks.is_some_and(|tracks| tracks.0.values().any(|track| track.level));
        let kinds = cx
            .mode_section::<SlotKinds>()
            .map_or(&[][..], SlotKinds::kinds);
        let leveled = kinds.iter().find(|kind| {
            kind.ranks
                .as_ref()
                .is_some_and(|ranks| ranks.levels().is_some())
        });
        match leveled {
            Some(kind) if !level_track => {
                let at = BookPlace::Section(SlotKinds::KEY);
                let problem = TrackError::RankLevels(kind.name.clone());
                Err(BookError::rule(PackageIndex::MODE, at, problem))
            }
            _ => Ok(()),
        }
    }

    /// The names of the mode's tracks.
    pub(crate) fn track_names(cx: &BookCx<'_>) -> Result<TrackNames, BookError> {
        Ok(TrackNames(Arc::clone(cx.book::<TrackBook>()?.names())))
    }
}

impl TrackBook {
    /// The mode's `[tracks]`; an error for more tracks than a match holds, or more than one that
    /// counts the units' level.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let none = BTreeMap::new();
        let tracks = cx.mode_section::<ModeTracksData>();
        let tracks = tracks.map_or(&none, |tracks| &tracks.0);
        Self::new(tracks).map_err(|error| {
            let at = BookPlace::Section(ModeTracksData::KEY);
            BookError::rule(PackageIndex::MODE, at, error)
        })
    }
}

impl TrackSet {
    /// The tracks of each unit type that stands and gains experience; an error for a track the
    /// mode does not declare.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let names = cx.book::<TrackBook>()?.names();
        let mut book = ByType::default();
        for unit in cx.standing() {
            let Some(tracks) = unit.data.section::<TypeTracksData>() else {
                continue;
            };
            let set = tracks.0.iter().map(|track| {
                let named = names.named(track.as_str());
                named.ok_or_else(|| {
                    let at = BookPlace::UnitType(unit.place());
                    BookError::unknown(unit.package, at, UnknownName::new(NameKind::Track, track))
                })
            });
            let set: Self = set.collect::<Result<_, BookError>>()?;
            if !tracks.0.is_empty() {
                book.set(unit.unit_type, set);
            }
        }
        Ok(book)
    }
}
