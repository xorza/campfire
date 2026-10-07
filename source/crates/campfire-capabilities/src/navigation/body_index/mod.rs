use std::cmp::Ordering;
use std::mem;
use std::ops::{ControlFlow, Range};

use bevy_ecs::resource::Resource;
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};

use crate::navigation::segment::Segment;
use crate::navigation::walker::Walker;
use crate::units::body::Body;
use crate::units::layer::Layer;
use crate::values::body_box::BodyBox;
use crate::values::grid::Grid;
use crate::values::row_directory::{RowDirectory, RowEntries};
use crate::values::shape::Shape;

/// Bodies that stand, by layer and by the square buckets their bounding boxes cover, so a query
/// sees only the bodies of its layer. As a resource it holds the
/// static bodies, those of the living units that cannot walk: collision finds a walker's static
/// contacts in it, and the pathing grid the static bodies near one that changed. Steering keeps
/// another for the units that stand this tick. A bucket is twice the widest walker's radius wide,
/// so a walker's box covers at most four; any width finds the same bodies. Derived from the
/// bodies, not state: each change of them changes only the buckets of the bodies it touched.
#[derive(Resource, Debug)]
pub(crate) struct BodyIndex {
    bucket: Num,
    /// The bodies, by stable id.
    bodies: Vec<IndexedBody>,
    /// Each body once in each bucket its box covers, sorted, and where each row of buckets starts.
    entries: Vec<Entry>,
    rows: RowDirectory<Layer>,
    /// The bodies the last update took away, and those it put in, by stable id: a body that
    /// changed is in both.
    removed: Vec<IndexedBody>,
    added: Vec<IndexedBody>,
    /// The new bodies' entries, and the entries they merge into, kept between updates.
    fresh: Vec<Entry>,
    merged: Vec<Entry>,
}

/// A unit's body as an index of bodies that stand sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IndexedBody {
    pub(crate) id: StableId,
    pub(crate) at: Position,
    pub(crate) shape: Shape,
    pub(crate) layer: Layer,
}

/// A body in one bucket of its layer, and whether the bucket is in the first row and the first
/// column of the body's buckets. Entries order by their bucket, then their body's stable id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Entry {
    bucket: BucketKey,
    body: IndexedBody,
    first_row: bool,
    first_column: bool,
}

/// A bucket of a layer, packed so that buckets order as their layer, row and column do: the
/// layer in the top byte, the row offset into the next 56 bits, the column offset into the low
/// 64, so a search compares one number, not three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct BucketKey(u128);

/// The buckets a box covers along one axis, `low` to `high`, both in.
#[derive(Debug, Clone, Copy)]
struct Buckets {
    low: i64,
    high: i64,
}

impl Entry {
    /// Its place in the order of entries.
    const fn order(&self) -> (BucketKey, StableId) {
        (self.bucket, self.body.id)
    }
}

impl BucketKey {
    /// The rows that fit, from −2⁵⁵ to 2⁵⁵ − 1. A body's rows lie within ±2⁴⁵, as positions lie
    /// within ±2⁴⁴ bits, a radius within 64 m, and a bucket is at least 2 bits wide; a search's
    /// rows past them clamp to the ends, whose buckets hold no body.
    const ROWS: i64 = 1 << 55;

    /// Its layer and row.
    const fn layer_row(self) -> (Layer, i64) {
        let high = (self.0 >> 64) as u64;
        let row = (high & ((1 << 56) - 1)).cast_signed() - BucketKey::ROWS;
        (Layer::new((high >> 56) as u8), row)
    }

    const fn new(layer: Layer, row: i64, column: i64) -> BucketKey {
        let row = if row < -BucketKey::ROWS {
            0
        } else if row >= BucketKey::ROWS {
            2 * BucketKey::ROWS - 1
        } else {
            row + BucketKey::ROWS
        };
        let high = (layer.index() as u64) << 56 | row.cast_unsigned();
        let low = column.cast_unsigned() ^ 1 << 63;
        BucketKey((high as u128) << 64 | low as u128)
    }
}

impl IndexedBody {
    /// The body `body` of unit `id`, standing at `at`.
    pub(crate) const fn of(id: StableId, at: Position, body: &Body) -> IndexedBody {
        IndexedBody {
            id,
            at,
            shape: body.shape(),
            layer: body.layer(),
        }
    }

    /// Whether `segment` comes closer than `reach` to the body: a walker of that radius along
    /// it would overlap it. Touching is not closer.
    pub(crate) fn comes_within(&self, segment: Segment, reach: Num) -> bool {
        self.shape
            .comes_within(self.at, segment.start(), segment.end(), reach)
    }

    /// Whether the insides of the body and of `body`, a box at `at`, share a point: touching is
    /// not overlap.
    pub(crate) fn overlaps_box(&self, at: Position, body: &BodyBox) -> bool {
        match &self.shape {
            Shape::Circle(radius) => body.nearest(at, self.at, *radius) == Ordering::Less,
            Shape::Box(other) => body.overlaps(at, other, self.at),
        }
    }

    /// Calls `mark` with each row's run of the cells of `grid` whose centers come closer than
    /// `reach` to the body, as a walker of that radius may not stand in them. Rows in order.
    pub(crate) fn spans_closer(&self, grid: &Grid, reach: Num, mark: impl FnMut(Range<usize>)) {
        match &self.shape {
            Shape::Circle(radius) => grid.spans_closer(self.at, reach + *radius, mark),
            Shape::Box(body) => grid.box_spans_closer(self.at, body, reach, mark),
        }
    }
}

impl BodyIndex {
    /// An empty index for walkers at most `widest` in radius, 0 when none has a body.
    pub(crate) fn new(widest: Num) -> BodyIndex {
        // With no walker of a body any width serves, and a bucket of 0 divides by 0.
        let widest = if widest > Num::ZERO {
            widest
        } else {
            Body::MAX_RADIUS
        };
        BodyIndex {
            bucket: widest + widest,
            bodies: Vec::new(),
            entries: Vec::new(),
            rows: RowDirectory::default(),
            removed: Vec::new(),
            added: Vec::new(),
            fresh: Vec::new(),
            merged: Vec::new(),
        }
    }

    /// An empty index with the same buckets.
    pub(crate) fn sibling(&self) -> BodyIndex {
        BodyIndex::new(self.bucket / 2)
    }

    /// Makes `bodies`, sorted by stable id, the index's bodies; whether they changed. The
    /// bodies the change took away and put in stay for `removed` and `added`.
    pub(crate) fn update(&mut self, bodies: &[IndexedBody]) -> bool {
        debug_assert!(bodies.is_sorted_by_key(|body| body.id));
        self.removed.clear();
        self.added.clear();
        if self.bodies == bodies {
            return false;
        }
        let (mut old, mut new) = (0, 0);
        loop {
            match (self.bodies.get(old), bodies.get(new)) {
                (Some(&was), Some(&is)) => match was.id.cmp(&is.id) {
                    Ordering::Less => {
                        self.removed.push(was);
                        old += 1;
                    }
                    Ordering::Greater => {
                        self.added.push(is);
                        new += 1;
                    }
                    Ordering::Equal => {
                        if was != is {
                            self.removed.push(was);
                            self.added.push(is);
                        }
                        old += 1;
                        new += 1;
                    }
                },
                (Some(&was), None) => {
                    self.removed.push(was);
                    old += 1;
                }
                (None, Some(&is)) => {
                    self.added.push(is);
                    new += 1;
                }
                (None, None) => break,
            }
        }
        let removed = &self.removed;
        self.entries.retain(|entry| {
            removed
                .binary_search_by_key(&entry.body.id, |body| body.id)
                .is_err()
        });
        self.fresh.clear();
        for body in &self.added {
            let extent = body.shape.extent();
            let rows = BodyIndex::buckets(self.bucket, body.at.get().z, extent[1]);
            let columns = BodyIndex::buckets(self.bucket, body.at.get().x, extent[0]);
            for row in rows.low..=rows.high {
                debug_assert!(row.abs() < BucketKey::ROWS, "a body's rows fit a key");
                self.fresh
                    .extend((columns.low..=columns.high).map(|column| Entry {
                        bucket: BucketKey::new(body.layer, row, column),
                        body: *body,
                        first_row: row == rows.low,
                        first_column: column == columns.low,
                    }));
            }
        }
        self.fresh.sort_unstable_by_key(Entry::order);
        self.merged.clear();
        self.merged
            .reserve_exact(self.entries.len() + self.fresh.len());
        let (mut kept, mut put) = (0, 0);
        while kept < self.entries.len() && put < self.fresh.len() {
            if self.entries[kept].order() < self.fresh[put].order() {
                self.merged.push(self.entries[kept]);
                kept += 1;
            } else {
                self.merged.push(self.fresh[put]);
                put += 1;
            }
        }
        self.merged.extend_from_slice(&self.entries[kept..]);
        self.merged.extend_from_slice(&self.fresh[put..]);
        mem::swap(&mut self.entries, &mut self.merged);
        let rows = self.entries.iter().map(|entry| entry.bucket.layer_row());
        self.rows.rebuild(rows);
        self.bodies.clear();
        self.bodies.extend_from_slice(bodies);
        true
    }

    pub(crate) const fn removed(&self) -> &[IndexedBody] {
        self.removed.as_slice()
    }

    pub(crate) const fn added(&self) -> &[IndexedBody] {
        self.added.as_slice()
    }

    /// How many static bodies the index holds.
    pub(crate) const fn len(&self) -> usize {
        self.bodies.len()
    }

    /// Calls `visit` once with each body of `layer` whose bounding box meets the square `reach`
    /// from `at` on each side, on the ground plane: every body of the layer that comes within
    /// `reach` of `at`, and some that do not. In order of the buckets, row by row.
    pub(crate) fn near(
        &self,
        layer: Layer,
        at: Vec3,
        reach: Num,
        mut visit: impl FnMut(&IndexedBody),
    ) {
        let rows = BodyIndex::buckets(self.bucket, at.z, reach);
        let columns = BodyIndex::buckets(self.bucket, at.x, reach);
        let all = self.meeting(layer, rows, columns, |body| {
            visit(body);
            ControlFlow::Continue(())
        });
        debug_assert!(
            all.is_continue(),
            "a visit that never breaks meets every body"
        );
    }

    /// Whether a body of `walker`'s layer comes closer to `segment` than the walker's radius,
    /// exactly: whether the walker along it would overlap one.
    pub(crate) fn blocks(&self, segment: Segment, walker: Walker) -> bool {
        let radius = walker.radius;
        let (from, to) = (segment.start().get(), segment.end().get());
        let span = |a: Num, b: Num| Buckets {
            low: (a.min(b) - radius)
                .to_bits()
                .div_euclid(self.bucket.to_bits()),
            high: (a.max(b) + radius)
                .to_bits()
                .div_euclid(self.bucket.to_bits()),
        };
        let met = self.meeting(
            walker.layer,
            span(from.z, to.z),
            span(from.x, to.x),
            |body| {
                if body.comes_within(segment, radius) {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        );
        met.is_break()
    }

    /// Calls `visit` once with each body of `layer` in the buckets of `rows` and `columns`, in
    /// the first of them its own buckets share, until it breaks: that bucket is in the first row
    /// of the search's or of the body's buckets, and in the first column of either.
    fn meeting(
        &self,
        layer: Layer,
        rows: Buckets,
        columns: Buckets,
        mut visit: impl FnMut(&IndexedBody) -> ControlFlow<()>,
    ) -> ControlFlow<()> {
        let Some((first, last)) = self.rows.span(layer) else {
            return ControlFlow::Continue(());
        };
        for row in rows.low.max(first)..=rows.high.min(last) {
            let Some(found) = self.rows.row(layer, row) else {
                continue;
            };
            let (RowEntries::Row(entries) | RowEntries::Layer(entries)) = found;
            let entries = &self.entries[entries];
            let (low, high) = (
                BucketKey::new(layer, row, columns.low),
                BucketKey::new(layer, row, columns.high),
            );
            let start = entries.partition_point(|entry| entry.bucket < low);
            let run = entries[start..]
                .iter()
                .take_while(|entry| entry.bucket <= high);
            for entry in run {
                let first = (row == rows.low || entry.first_row)
                    && (entry.bucket == low || entry.first_column);
                if first {
                    visit(&entry.body)?;
                }
            }
        }
        ControlFlow::Continue(())
    }

    /// The buckets of width `bucket` that cover `center` less `reach` to `center` plus `reach`.
    const fn buckets(bucket: Num, center: Num, reach: Num) -> Buckets {
        let (center, reach, bucket) = (center.to_bits(), reach.to_bits(), bucket.to_bits());
        Buckets {
            low: (center - reach).div_euclid(bucket),
            high: (center + reach).div_euclid(bucket),
        }
    }
}

#[cfg(test)]
mod tests;
