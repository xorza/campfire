use std::ops::Range;

use crate::units::row_fill::FillRow;
use crate::units::view_column::ViewColumns;

/// A source's part of a read: whether it fills every row, and the run of the read before's rows
/// it keeps next, which grows while the rows it keeps follow one another there.
#[derive(Debug, Clone)]
pub(crate) struct SourceRead {
    refill: bool,
    kept: Option<Range<usize>>,
}

/// The sources' parts of a read, and the run of rows that every source keeps, which each takes
/// at the next row one of them fills, or as the read ends.
#[derive(Debug, Default)]
pub(crate) struct SourceReads {
    each: Vec<SourceRead>,
    clean: Option<Range<usize>>,
}

impl SourceReads {
    /// Starts a read whose sources each fill every row when they `refill`.
    pub(crate) fn begin(&mut self, refill: impl IntoIterator<Item = bool>) {
        self.each.clear();
        self.each.extend(
            refill
                .into_iter()
                .map(|refill| SourceRead { refill, kept: None }),
        );
        self.clean = None;
    }

    /// Whether any source fills every row.
    pub(crate) fn any_refills(&self) -> bool {
        self.each.iter().any(|read| read.refill)
    }

    /// Each source's part, in the sources' order.
    pub(crate) fn each(&mut self) -> &mut [SourceRead] {
        &mut self.each
    }

    /// Keeps row `at` of the read before for every source: it joins their run, or each source
    /// takes the run, and the row starts the next.
    pub(crate) fn keep_all(
        &mut self,
        sources: &[Box<dyn FillRow>],
        columns: &mut ViewColumns,
        at: usize,
    ) {
        match &mut self.clean {
            Some(run) if run.end == at => run.end += 1,
            _ => {
                self.take_clean(sources, columns);
                self.clean = Some(at..at + 1);
            }
        }
    }

    /// Gives each source the run every source keeps.
    pub(crate) fn take_clean(&mut self, sources: &[Box<dyn FillRow>], columns: &mut ViewColumns) {
        if let Some(run) = self.clean.take() {
            for (source, read) in sources.iter().zip(&mut self.each) {
                read.keep(source.as_ref(), columns, run.clone());
            }
        }
    }

    /// Adds what each source kept to its column, as the read ends.
    pub(crate) fn finish(&mut self, sources: &[Box<dyn FillRow>], columns: &mut ViewColumns) {
        self.take_clean(sources, columns);
        for (source, read) in sources.iter().zip(&mut self.each) {
            read.flush(source.as_ref(), columns);
        }
    }
}

impl SourceRead {
    /// Whether it fills every row.
    pub(crate) const fn refills(&self) -> bool {
        self.refill
    }

    /// Keeps the rows `rows` of the read before: they join the run, or `source` adds the run
    /// to its column, and they start the next.
    pub(crate) fn keep(
        &mut self,
        source: &dyn FillRow,
        columns: &mut ViewColumns,
        rows: Range<usize>,
    ) {
        match &mut self.kept {
            Some(run) if run.end == rows.start => run.end = rows.end,
            run => {
                if let Some(run) = run.replace(rows) {
                    source.keep(columns, run);
                }
            }
        }
    }

    /// Adds the run kept so far to `source`'s column: before a row it fills, and as the read
    /// ends.
    pub(crate) fn flush(&mut self, source: &dyn FillRow, columns: &mut ViewColumns) {
        if let Some(run) = self.kept.take() {
            source.keep(columns, run);
        }
    }
}
