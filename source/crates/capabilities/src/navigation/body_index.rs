use std::cmp::Ordering;
use std::mem;

use bevy_ecs::resource::Resource;
use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};

use crate::units::body::Body;
use crate::values::segment::Segment;

/// Bodies that stand, by the square buckets their bounding boxes cover. As a resource it holds the
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
    /// Each body once in each bucket its box covers, sorted.
    entries: Vec<Entry>,
    /// The bodies the last update took away, and those it put in, by stable id: a body that
    /// changed is in both.
    removed: Vec<IndexedBody>,
    added: Vec<IndexedBody>,
    /// The new bodies' entries, and the entries they merge into, kept between updates.
    fresh: Vec<Entry>,
    merged: Vec<Entry>,
    changes: u64,
}

/// A unit's body as an index of bodies that stand sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IndexedBody {
    pub(crate) id: StableId,
    pub(crate) at: Position,
    pub(crate) radius: Num,
}

/// A body in one bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    row: i64,
    column: i64,
    id: StableId,
}

/// The buckets a box covers along one axis, `low` to `high`, both in.
#[derive(Debug, Clone, Copy)]
struct Buckets {
    low: i64,
    high: i64,
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
            removed: Vec::new(),
            added: Vec::new(),
            fresh: Vec::new(),
            merged: Vec::new(),
            changes: 0,
        }
    }

    /// An empty index with the same buckets.
    pub(crate) fn sibling(&self) -> BodyIndex {
        BodyIndex::new(Num::from_bits(self.bucket.to_bits() / 2))
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
                .binary_search_by_key(&entry.id, |body| body.id)
                .is_err()
        });
        self.fresh.clear();
        for body in &self.added {
            let rows = BodyIndex::buckets(self.bucket, body.at.get().z, body.radius);
            let columns = BodyIndex::buckets(self.bucket, body.at.get().x, body.radius);
            for row in rows.low..=rows.high {
                self.fresh
                    .extend((columns.low..=columns.high).map(|column| Entry {
                        row,
                        column,
                        id: body.id,
                    }));
            }
        }
        self.fresh.sort_unstable();
        self.merged.clear();
        self.merged
            .reserve_exact(self.entries.len() + self.fresh.len());
        let (mut kept, mut put) = (0, 0);
        while kept < self.entries.len() && put < self.fresh.len() {
            if self.entries[kept] < self.fresh[put] {
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
        self.bodies.clear();
        self.bodies.extend_from_slice(bodies);
        self.changes += 1;
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

    /// Calls `visit` once with each static body whose bounding box meets the square `reach` from
    /// `at` on each side, on the ground plane: every body that comes within `reach` of `at`, and
    /// some that do not. In order of the buckets, row by row.
    pub(crate) fn near(&self, at: Vec3, reach: Num, visit: impl FnMut(&IndexedBody)) {
        let rows = BodyIndex::buckets(self.bucket, at.z, reach);
        let columns = BodyIndex::buckets(self.bucket, at.x, reach);
        self.meeting(rows, columns, visit);
    }

    /// Whether a static body comes closer to `segment` than its radius and `radius` together,
    /// exactly: whether a walker of `radius` along it would overlap one.
    pub(crate) fn blocks(&self, segment: Segment, radius: Num) -> bool {
        let (from, to) = (segment.start().get(), segment.end().get());
        let span = |a: Num, b: Num| Buckets {
            low: (a.min(b) - radius)
                .to_bits()
                .div_euclid(self.bucket.to_bits()),
            high: (a.max(b) + radius)
                .to_bits()
                .div_euclid(self.bucket.to_bits()),
        };
        let mut blocked = false;
        self.meeting(span(from.z, to.z), span(from.x, to.x), |body| {
            blocked = blocked || segment.comes_within(body.at, radius + body.radius);
        });
        blocked
    }

    /// Whether a static body blocks a walker of `radius` on its way from `from` along
    /// `waypoints`.
    pub(crate) fn blocks_route(&self, from: Position, waypoints: &[Position], radius: Num) -> bool {
        let mut at = from;
        waypoints.iter().any(|&next| {
            let leg = Segment::new(at, next);
            at = next;
            self.blocks(leg, radius)
        })
    }

    /// How many updates changed the static bodies: it changes whenever they do.
    pub(crate) const fn changes(&self) -> u64 {
        self.changes
    }

    /// Calls `visit` once with each body in the buckets of `rows` and `columns`, in the first of
    /// them its own buckets share.
    fn meeting(&self, rows: Buckets, columns: Buckets, mut visit: impl FnMut(&IndexedBody)) {
        for row in rows.low..=rows.high {
            let start = self
                .entries
                .partition_point(|entry| (entry.row, entry.column) < (row, columns.low));
            let run = self.entries[start..]
                .iter()
                .take_while(|entry| entry.row == row && entry.column <= columns.high);
            for entry in run {
                let body = &self.bodies[self
                    .bodies
                    .binary_search_by_key(&entry.id, |body| body.id)
                    .expect("an entry's body is in the index")];
                let own_rows = BodyIndex::buckets(self.bucket, body.at.get().z, body.radius);
                let own_columns = BodyIndex::buckets(self.bucket, body.at.get().x, body.radius);
                if row == rows.low.max(own_rows.low)
                    && entry.column == columns.low.max(own_columns.low)
                {
                    visit(body);
                }
            }
        }
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
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    fn num(value: i64) -> Num {
        Num::from_int(value).unwrap()
    }

    fn body(id: StableId, x: i64, z: i64, radius: Num) -> IndexedBody {
        IndexedBody {
            id,
            at: Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap(),
            radius,
        }
    }

    fn near(index: &BodyIndex, x: i64, z: i64, reach: Num) -> Vec<StableId> {
        let mut found = Vec::new();
        index.near(Vec3::new(num(x), Num::ZERO, num(z)), reach, |body| {
            found.push(body.id);
        });
        found
    }

    #[test]
    fn the_index_finds_each_body_near_once_and_follows_its_changes() {
        // Buckets of 2 m, for walkers of 1 m.
        let mut index = BodyIndex::new(Num::ONE);
        let mut ids = IdAllocator::default();
        let (wide, small, far) = (ids.allocate(), ids.allocate(), ids.allocate());
        // A body of 5 m at the origin covers buckets −3 to 2 on both axes, 36 of them; one of
        // 1 m at (3, 3) covers buckets 1 to 2, 4 of them; one at (40, 0), buckets 19 to 20 along x
        // and −1 to 0 along z.
        let bodies = [
            body(wide, 0, 0, num(5)),
            body(small, 3, 3, Num::ONE),
            body(far, 40, 0, Num::ONE),
        ];
        assert!(index.update(&bodies));
        assert_eq!(index.entries.len(), 36 + 4 + 4);
        assert_eq!(index.added(), bodies);
        assert!(!index.update(&bodies));
        assert_eq!(index.added(), []);

        // Around (3, 3) by 1 m, buckets 1 to 2: the wide body and the small one share all four,
        // and each is found once. Around (0, 0) by 1 m, buckets −1 to 0: only the wide body. The
        // box of (40, 0) reaches no bucket between. Around (20, 0) by 20 m, rows −10 to 10, each
        // is found in its first row there: the wide body in row −3, the far one in −1, the small
        // one in 1.
        assert_eq!(near(&index, 3, 3, Num::ONE), [wide, small]);
        assert_eq!(near(&index, 0, 0, Num::ONE), [wide]);
        assert_eq!(near(&index, 20, 0, Num::ONE), []);
        assert_eq!(near(&index, 20, 0, num(20)), [wide, far, small]);

        // The wide body moves to (40, 20): it is taken away and put in again; the small body
        // goes, and a new one comes.
        let new = ids.allocate();
        let moved = [
            body(wide, 40, 20, num(5)),
            body(far, 40, 0, Num::ONE),
            body(new, -10, -10, Num::ONE),
        ];
        assert!(index.update(&moved));
        assert_eq!(index.removed(), [bodies[0], bodies[1]]);
        assert_eq!(index.added(), [moved[0], moved[2]]);
        assert_eq!(near(&index, 3, 3, Num::ONE), []);
        assert_eq!(near(&index, 40, 16, Num::ONE), [wide]);
        assert_eq!(near(&index, -10, -10, Num::ONE), [new]);

        // The same bodies put in at once give the same entries.
        let mut again = BodyIndex::new(Num::ONE);
        again.update(&moved);
        assert_eq!(again.entries, index.entries);

        assert!(index.update(&[]));
        assert_eq!(index.entries, []);
        assert_eq!(index.removed(), moved);
    }
}
