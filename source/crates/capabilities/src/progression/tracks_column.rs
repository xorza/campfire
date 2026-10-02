use crate::progression::track_book::TrackBook;
use crate::progression::track_set::TrackSet;
use crate::scripts::error::{ApiError, Checked};
use crate::units::script_view::View;
use crate::units::track_id::TrackId;
use crate::units::view_column::ViewColumn;

/// What progression adds to the script view: the tracks the mode declares, and the tracks each
/// unit has, a row each.
#[derive(Debug, Default)]
pub(crate) struct TracksColumn {
    book: TrackBook,
    rows: Vec<TrackSet>,
}

impl ViewColumn for TracksColumn {
    fn clear(&mut self) {
        self.rows.clear();
    }

    fn rows(&self) -> usize {
        self.rows.len()
    }
}

impl TracksColumn {
    /// Adds the row of a unit that has `tracks`.
    pub(crate) fn push(&mut self, tracks: TrackSet) {
        self.rows.push(tracks);
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

    /// Whether the unit in row `row` of the view has `track`.
    pub(crate) fn has(view: &View, row: usize, track: TrackId) -> bool {
        view.column(|column: &TracksColumn| column.rows[row].contains(track))
            .unwrap_or(false)
    }
}
