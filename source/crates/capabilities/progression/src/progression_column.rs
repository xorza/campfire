use std::ops::Range;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_script::{
    AnyKeptRows, ApiError, Checked, ColumnRows, KeptRows, RowFill, RunMove, View, ViewColumn,
};
use campfire_sim::StableId;
use campfire_stats::Level;
use campfire_values::TrackId;

use crate::experience::{Experience, TrackXp};
use crate::points::Points;
use crate::track_book::TrackBook;

/// The parts of a unit progression reads into its row: its experience and its points.
pub type RowReads = (Option<&'static Experience>, Option<&'static Points>);

/// What progression adds to the script view: the tracks the mode declares, and each unit's
/// tracks, its experience and level on each, and its points, a row each.
#[derive(Debug, Default)]
pub struct ProgressionColumn {
    book: TrackBook,
    rows: KeptRows<ProgressionRows>,
}

/// The rows of one read of the progression column.
#[derive(Debug, Default, PartialEq, Eq)]
struct ProgressionRows {
    rows: Vec<TracksRow>,
    held: Vec<TrackXp>,
}

/// A unit's progress as the view read it: its run of tracks in `held`, in the order of their
/// ids, and its points, none without the `level` track.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TracksRow {
    held: Range<u32>,
    points: Option<Points>,
}

impl ViewColumn for ProgressionColumn {
    fn kept(&self) -> &dyn AnyKeptRows {
        &self.rows
    }

    fn kept_mut(&mut self) -> &mut dyn AnyKeptRows {
        &mut self.rows
    }

    fn begin(&mut self, world: &World) -> bool {
        let book = world.resource::<TrackBook>();
        if !book.same(&self.book) {
            self.book = book.clone();
        }
        false
    }
}

impl ColumnRows for ProgressionRows {
    fn clear(&mut self) {
        self.rows.clear();
        self.held.clear();
    }

    fn push_from(&mut self, from: &Self, rows: Range<usize>) {
        let (first, end) = (
            from.rows[rows.start].held.start,
            from.rows[rows.end - 1].held.end,
        );
        let moved = RunMove::new(first, self.held.len());
        self.rows
            .extend(from.rows[rows].iter().map(|row| TracksRow {
                held: moved.of(&row.held),
                points: row.points,
            }));
        self.held
            .extend_from_slice(&from.held[first as usize..end as usize]);
    }

    fn len(&self) -> usize {
        self.rows.len()
    }
}

impl ProgressionRows {
    /// The run of tracks of the unit in row `row`.
    fn held(&self, row: usize) -> &[TrackXp] {
        let run = &self.rows[row].held;
        &self.held[run.start as usize..run.end as usize]
    }
}

impl ProgressionColumn {
    /// Adds the row of a unit with `experience`, or with none, and with `points`.
    pub fn push(&mut self, experience: Option<&Experience>, points: Option<&Points>) {
        let rows = self.rows.now_mut();
        let start = u32::try_from(rows.held.len()).expect("tracks fit u32");
        rows.held
            .extend(experience.into_iter().flat_map(Experience::iter));
        let end = u32::try_from(rows.held.len()).expect("tracks fit u32");
        rows.rows.push(TracksRow {
            held: start..end,
            points: points.copied(),
        });
    }

    /// Whether `id` has `track`; false for a unit the view does not hold.
    pub(crate) fn has(view: &View, id: StableId, track: TrackId) -> bool {
        let Some(row) = view.row_index(id) else {
            return false;
        };
        view.column(|column: &ProgressionColumn| column.find(row, track).is_some())
            .unwrap_or(false)
    }

    /// The experience of the unit in row `row` on `track`; an error for a track it does not
    /// have.
    pub(crate) fn xp(view: &View, row: usize, track: TrackId) -> Checked<Num> {
        Ok(ProgressionColumn::held(view, row, track)?.xp)
    }

    /// The level of the unit in row `row` on `track`, as `xp` refuses.
    pub(crate) fn level(view: &View, row: usize, track: TrackId) -> Checked<Level> {
        Ok(ProgressionColumn::held(view, row, track)?.level)
    }

    /// The unspent points of the unit in row `row`; an error for a unit without the `level`
    /// track.
    pub(crate) fn points(view: &View, row: usize) -> Checked<Points> {
        let points = view.column(|column: &ProgressionColumn| column.rows.now().rows[row].points);
        Ok(points.flatten().ok_or_else(|| ApiError::NoTrack.fail())?)
    }

    fn held(view: &View, row: usize, track: TrackId) -> Checked<TrackXp> {
        let held = view.column(|column: &ProgressionColumn| column.find(row, track));
        Ok(held.flatten().ok_or_else(|| ApiError::NoTrack.fail())?)
    }

    /// The unit in row `row`'s progress on `track`, when it has the track.
    fn find(&self, row: usize, track: TrackId) -> Option<TrackXp> {
        let held = self.rows.now().held(row);
        held.iter().find(|held| held.track == track).copied()
    }

    /// Fills a row of the script view with a unit's tracks, its progress on each, and its points.
    pub(super) fn fill_row(
        (experience, points): ROQueryItem<'_, '_, RowReads>,
        fill: &mut RowFill<'_, ProgressionColumn>,
    ) {
        fill.column.push(experience, points);
    }
}
