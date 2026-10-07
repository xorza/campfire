use std::ops::Range;

/// Where a run of entries sorted by layer, then by row, starts each row, layer by layer, so a
/// search finds a row's entries at once rather than by a search of all of them, and skips the
/// rows past a layer's first and last. A layer is any key the entries group by. A layer whose
/// rows spread far wider than its entries keeps only its run of entries, which a search then
/// searches.
#[derive(Debug)]
pub(crate) struct RowDirectory<L> {
    layers: Vec<LayerRows<L>>,
    /// Each layer's row starts, layer after layer.
    starts: Vec<u32>,
}

/// One layer's entries, its first and last row, and its run of row starts: each row's first
/// entry, and one past the last row's last, when it keeps them.
#[derive(Debug, Clone)]
struct LayerRows<L> {
    layer: L,
    entries: Range<u32>,
    first: i64,
    last: i64,
    starts: Option<Range<u32>>,
}

/// What a search finds of a layer's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RowEntries {
    /// The row's entries, which the directory found.
    Row(Range<usize>),
    /// The layer's entries, which hold the row's, as the layer keeps no starts.
    Layer(Range<usize>),
}

impl<L> Default for RowDirectory<L> {
    fn default() -> RowDirectory<L> {
        RowDirectory {
            layers: Vec::new(),
            starts: Vec::new(),
        }
    }
}

impl<L: Copy + Eq> RowDirectory<L> {
    /// How much wider than its entries a layer's rows may spread for it to keep their starts.
    const SPREAD: u64 = 4;
    /// The rows a layer may always keep the starts of.
    const ROWS: u64 = 64;

    /// Builds it again from `rows`, the layer and the row of each entry, in the entries' order.
    pub(crate) fn rebuild(&mut self, rows: impl Iterator<Item = (L, i64)> + Clone) {
        self.layers.clear();
        self.starts.clear();
        let count = |at: usize| u32::try_from(at).expect("entries fit u32");
        let mut at = 0;
        let mut layers = rows.clone().peekable();
        while let Some(&(layer, first)) = layers.peek() {
            let start = at;
            let mut last = first;
            while let Some((_, row)) = layers.next_if(|&(other, _)| other == layer) {
                last = row;
                at += 1;
            }
            self.layers.push(LayerRows {
                layer,
                entries: count(start)..count(at),
                first,
                last,
                starts: None,
            });
        }
        let mut rows = rows.peekable();
        for layer in &mut self.layers {
            let entries = (layer.entries.end - layer.entries.start) as usize;
            let spread = RowDirectory::<L>::SPREAD * entries as u64 + RowDirectory::<L>::ROWS;
            if layer.last.abs_diff(layer.first) >= spread {
                rows.by_ref().take(entries).for_each(drop);
                continue;
            }
            let begin = count(self.starts.len());
            let mut entry = layer.entries.start;
            for row in layer.first..=layer.last {
                self.starts.push(entry);
                while rows
                    .next_if(|&(other, at)| other == layer.layer && at == row)
                    .is_some()
                {
                    entry += 1;
                }
            }
            self.starts.push(entry);
            layer.starts = Some(begin..count(self.starts.len()));
        }
    }

    /// The first and the last row of `layer`'s entries; `None` with none.
    pub(crate) fn span(&self, layer: L) -> Option<(i64, i64)> {
        let rows = self.layers.iter().find(|rows| rows.layer == layer)?;
        Some((rows.first, rows.last))
    }

    /// The entries of `layer`'s row `row`; `None` when the layer has none there.
    pub(crate) fn row(&self, layer: L, row: i64) -> Option<RowEntries> {
        let rows = self.layers.iter().find(|rows| rows.layer == layer)?;
        if row < rows.first || row > rows.last {
            return None;
        }
        let Some(starts) = &rows.starts else {
            let entries = rows.entries.start as usize..rows.entries.end as usize;
            return Some(RowEntries::Layer(entries));
        };
        let at = starts.start as usize + usize::try_from(row - rows.first).expect("a row within");
        let (start, end) = (self.starts[at] as usize, self.starts[at + 1] as usize);
        (start < end).then_some(RowEntries::Row(start..end))
    }
}
