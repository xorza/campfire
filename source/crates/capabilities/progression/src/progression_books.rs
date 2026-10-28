use std::sync::Arc;

use campfire_actions::SlotKinds;
use campfire_declarations::{BookCx, BookError, BookPlace, Section};
use campfire_values::{Names, PackageIndex, PerkId, TrackId};

use crate::Progression;
use crate::error::TrackError;
use crate::mode_tracks_data::ModeTracksData;
use crate::perk_book::PerkBook;
use crate::track_book::TrackBook;

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

    /// The names of the mode's perks.
    pub(crate) fn perk_names(cx: &BookCx<'_>) -> Result<Arc<Names<PerkId>>, BookError> {
        Ok(Arc::clone(cx.book::<PerkBook>()?.names()))
    }

    /// The names of the mode's tracks.
    pub(crate) fn track_names(cx: &BookCx<'_>) -> Result<Arc<Names<TrackId>>, BookError> {
        Ok(Arc::clone(cx.book::<TrackBook>()?.names()))
    }
}
