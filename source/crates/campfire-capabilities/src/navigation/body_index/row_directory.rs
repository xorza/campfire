use std::ops::Range;

use crate::units::layer::Layer;

/// Where the body index's entries of each bucket row start, layer by layer, so a search finds a
/// row's entries at once rather than by a search of all of them, and skips the rows past a
/// layer's first and last. A layer whose rows spread far wider than its entries keeps only its
/// run of entries, which a search then searches.
#[derive(Debug, Default)]
pub(crate) struct RowDirectory {
    layers: Vec<LayerRows>,
    /// Each layer's row starts, layer after layer.
    starts: Vec<u32>,
}

/// One layer's entries, its first and last row, and its run of row starts: each row's first
/// entry, and one past the last row's last, when it keeps them.
#[derive(Debug, Clone)]
struct LayerRows {
    layer: Layer,
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

impl RowDirectory {
    /// How much wider than its entries a layer's rows may spread for it to keep their starts.
    const SPREAD: usize = 4;
    /// The rows a layer may always keep the starts of.
    const ROWS: usize = 64;

    /// Builds it again from `rows`, the layer and the row of each entry, in the entries' order.
    pub(crate) fn rebuild(&mut self, rows: impl Iterator<Item = (Layer, i64)> + Clone) {
        self.layers.clear();
        self.starts.clear();
        let mut at = 0_u32;
        let mut layers = rows.clone().peekable();
        while let Some(&(layer, first)) = layers.peek() {
            let start = at;
            let mut last = first;
            while let Some(&(other, row)) = layers.peek()
                && other == layer
            {
                last = row;
                at += 1;
                layers.next();
            }
            self.layers.push(LayerRows {
                layer,
                entries: start..at,
                first,
                last,
                starts: None,
            });
        }
        for at in 0..self.layers.len() {
            let layer = self.layers[at].clone();
            let span = usize::try_from(layer.last - layer.first).expect("rows ascend") + 1;
            let count = (layer.entries.end - layer.entries.start) as usize;
            if span > RowDirectory::SPREAD * count + RowDirectory::ROWS {
                continue;
            }
            let begin = u32::try_from(self.starts.len()).expect("starts fit u32");
            let mut entry = layer.entries.start;
            let mut layer_rows = rows
                .clone()
                .skip(layer.entries.start as usize)
                .take(count)
                .peekable();
            for row in layer.first..=layer.last {
                self.starts.push(entry);
                while layer_rows.next_if(|&(_, at)| at == row).is_some() {
                    entry += 1;
                }
            }
            self.starts.push(entry);
            let end = u32::try_from(self.starts.len()).expect("starts fit u32");
            self.layers[at].starts = Some(begin..end);
        }
    }

    /// The first and the last row of `layer`'s entries; `None` with none.
    pub(crate) fn span(&self, layer: Layer) -> Option<(i64, i64)> {
        let rows = self.layers.iter().find(|rows| rows.layer == layer)?;
        Some((rows.first, rows.last))
    }

    /// The entries of `layer`'s row `row`; `None` when the layer has none there.
    pub(crate) fn row(&self, layer: Layer, row: i64) -> Option<RowEntries> {
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
