use std::ops::Range;

use campfire_math::Num;
use campfire_sim::StableId;

use crate::progression::experience::Experience;
use crate::progression::points::Points;
use crate::progression::track_book::TrackBook;
use crate::scripts::error::{ApiError, Checked};
use crate::stats::level::Level;
use crate::units::script_view::View;
use crate::units::track_id::TrackId;
use crate::units::view_column::ViewColumn;

/// What progression adds to the script view: the tracks the mode declares, and each unit's
/// tracks, its experience and level on each, and its points, a row each.
#[derive(Debug, Default)]
pub(crate) struct TracksColumn {
    book: TrackBook,
    rows: Vec<TracksRow>,
    held: Vec<HeldTrack>,
}

/// A unit's progress as the view read it: its run of tracks in `held`, in the order of their
/// ids, and its points, none without the `level` track.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TracksRow {
    held: Range<u32>,
    points: Option<Points>,
}

/// A unit's experience and level on one of its tracks, the unit's level on the `level` track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeldTrack {
    track: TrackId,
    xp: Num,
    level: Level,
}

impl ViewColumn for TracksColumn {
    fn clear(&mut self) {
        self.rows.clear();
        self.held.clear();
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }
}

impl TracksColumn {
    /// Adds the row of a unit with `experience`, or with none, at `level`, the level of its
    /// `level` track, with `points`.
    pub(crate) fn push(
        &mut self,
        experience: Option<&Experience>,
        level: Option<&Level>,
        points: Option<&Points>,
    ) {
        let start = u32::try_from(self.held.len()).expect("tracks fit u32");
        self.held.extend(
            experience
                .into_iter()
                .flat_map(Experience::iter)
                .map(|held| {
                    let level = held.level.unwrap_or_else(|| {
                        *level.expect("a unit with the `level` track has a level")
                    });
                    HeldTrack {
                        track: held.track,
                        xp: held.xp,
                        level,
                    }
                }),
        );
        let end = u32::try_from(self.held.len()).expect("tracks fit u32");
        self.rows.push(TracksRow {
            held: start..end,
            points: points.copied(),
        });
    }

    /// Shares the tracks the mode declares with the view, which names them to scripts.
    pub(crate) fn share(view: &View, book: TrackBook) {
        view.set_track_names(book.names());
        view.column_mut(|column: &mut TracksColumn| {
            column.book = book;
        });
    }

    /// The track `name`; an error for one the mode does not declare.
    pub(crate) fn track_named(view: &View, name: &str) -> Checked<TrackId> {
        let found = view.column(|column: &TracksColumn| column.book.named(name));
        Ok(found
            .flatten()
            .ok_or_else(|| ApiError::UnknownTrack.fail())?)
    }

    /// Whether `unit` has `track`; false for a unit the view does not hold.
    pub(crate) fn has(view: &View, unit: StableId, track: TrackId) -> bool {
        let Some(row) = view.row_index(unit) else {
            return false;
        };
        view.column(|column: &TracksColumn| column.find(row, track).is_some())
            .unwrap_or(false)
    }

    /// The experience of the unit in row `row` on `track`; an error for a track it does not
    /// have.
    pub(crate) fn xp(view: &View, row: usize, track: TrackId) -> Checked<Num> {
        Ok(TracksColumn::held(view, row, track)?.xp)
    }

    /// The level of the unit in row `row` on `track`, as `xp` refuses.
    pub(crate) fn level(view: &View, row: usize, track: TrackId) -> Checked<Level> {
        Ok(TracksColumn::held(view, row, track)?.level)
    }

    /// The unspent points of the unit in row `row`; an error for a unit without the `level`
    /// track.
    pub(crate) fn points(view: &View, row: usize) -> Checked<Points> {
        let points = view.column(|column: &TracksColumn| column.rows[row].points);
        Ok(points.flatten().ok_or_else(|| ApiError::NoTrack.fail())?)
    }

    fn held(view: &View, row: usize, track: TrackId) -> Checked<HeldTrack> {
        let held = view.column(|column: &TracksColumn| column.find(row, track));
        Ok(held.flatten().ok_or_else(|| ApiError::NoTrack.fail())?)
    }

    /// The unit in row `row`'s progress on `track`, when it has the track.
    fn find(&self, row: usize, track: TrackId) -> Option<HeldTrack> {
        let run = &self.rows[row].held;
        let run = &self.held[run.start as usize..run.end as usize];
        run.iter().find(|held| held.track == track).copied()
    }
}
