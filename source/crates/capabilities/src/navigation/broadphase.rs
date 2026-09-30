use campfire_math::Num;

use crate::navigation::collider::Collider;
use crate::navigation::static_index::StaticIndex;

/// Finds the pairs of bodies that overlap. Two walkers come from a sort of the walkers by cell: a
/// cell is twice the widest walker's radius, so two that overlap sit in the same cell or in cells
/// side by side. Each occupied cell pairs its own walkers, and those of the cell to its right and
/// of the three below it, so each pair of cells is visited once; cursors that only move forward
/// find those cells. A sort, not a grid over the map, as a map may be wide and its bodies few. A
/// walker and a static body come from the static index, so a wide structure does not make the
/// cells wide, and two static bodies never part. The buffers stay between ticks, so a tick
/// allocates nothing once they have grown, and costs `n log n` and the pairs of bodies in cells
/// side by side.
#[derive(Debug, Default)]
pub(crate) struct Broadphase {
    /// Each collider's cell and index, sorted by cell, row by row, then by index.
    entries: Vec<Entry>,
    /// The occupied cells, in the order of `entries`, each with its run of entries.
    cells: Vec<Cell>,
    contacts: Vec<Contact>,
}

/// A collider's cell and its index among the colliders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    row: i64,
    column: i64,
    index: usize,
}

/// An occupied cell, and the run of `entries` in it.
#[derive(Debug, Clone, Copy)]
struct Cell {
    row: i64,
    column: i64,
    start: usize,
    end: usize,
}

/// Two colliders whose bodies overlap, by their indices, the first the lower.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Contact {
    pub(crate) first: usize,
    pub(crate) second: usize,
}

impl Broadphase {
    /// The pairs of `colliders`, sorted by stable id, that overlap as they stand now, in the order
    /// of their indices, first then second: the pairs a check of every pair finds, in its order.
    /// `statics` holds the colliders that may not be pushed.
    pub(crate) fn contacts(&mut self, colliders: &[Collider], statics: &StaticIndex) -> &[Contact] {
        self.entries.clear();
        self.cells.clear();
        self.contacts.clear();
        let Some(size) = Broadphase::cell(colliders) else {
            return &self.contacts;
        };
        let size = size.to_bits();
        let walkers = colliders
            .iter()
            .enumerate()
            .filter(|(_, collider)| collider.movable);
        self.entries
            .extend(walkers.clone().map(|(index, collider)| Entry {
                row: collider.at.z.to_bits().div_euclid(size),
                column: collider.at.x.to_bits().div_euclid(size),
                index,
            }));
        self.entries.sort_unstable();
        for (at, entry) in self.entries.iter().enumerate() {
            match self.cells.last_mut() {
                Some(cell) if (cell.row, cell.column) == (entry.row, entry.column) => {
                    cell.end = at + 1;
                }
                _ => self.cells.push(Cell {
                    row: entry.row,
                    column: entry.column,
                    start: at,
                    end: at + 1,
                }),
            }
        }
        let mut below = 0;
        for (at, &cell) in self.cells.iter().enumerate() {
            let run = &self.entries[cell.start..cell.end];
            for (offset, a) in run.iter().enumerate() {
                for b in &run[offset + 1..] {
                    Broadphase::check(colliders, a.index, b.index, &mut self.contacts);
                }
            }
            let right = self
                .cells
                .get(at + 1)
                .filter(|next| (next.row, next.column) == (cell.row, cell.column + 1));
            while self
                .cells
                .get(below)
                .is_some_and(|other| (other.row, other.column) < (cell.row + 1, cell.column - 1))
            {
                below += 1;
            }
            let under = self.cells[below..]
                .iter()
                .take_while(|other| (other.row, other.column) <= (cell.row + 1, cell.column + 1));
            for other in right.into_iter().chain(under) {
                for a in run {
                    for b in &self.entries[other.start..other.end] {
                        Broadphase::check(colliders, a.index, b.index, &mut self.contacts);
                    }
                }
            }
        }
        for (index, collider) in walkers {
            statics.near(collider.at, collider.radius, |body| {
                let other = colliders
                    .binary_search_by_key(&body.id, |collider| collider.id)
                    .expect("a static body of the index is among the colliders");
                debug_assert!(!colliders[other].movable);
                Broadphase::check(colliders, index, other, &mut self.contacts);
            });
        }
        self.contacts.sort_unstable();
        &self.contacts
    }

    /// The width of a cell: twice the widest radius of the colliders that may be pushed; `None`
    /// with none, as no two others part.
    fn cell(colliders: &[Collider]) -> Option<Num> {
        let widest = colliders
            .iter()
            .filter(|collider| collider.movable)
            .map(|collider| collider.radius)
            .max()?;
        Some(widest + widest)
    }

    /// Records the contact of colliders `a` and `b` when they overlap.
    fn check(colliders: &[Collider], a: usize, b: usize, contacts: &mut Vec<Contact>) {
        if colliders[a].overlaps(&colliders[b]) {
            contacts.push(Contact {
                first: a.min(b),
                second: a.max(b),
            });
        }
    }
}

#[cfg(any(test, feature = "bench"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_math::{Num, Vec3};
    use campfire_sim::{IdAllocator, Position};

    use crate::navigation::collider::Collider;
    use crate::navigation::static_index::{StaticBody, StaticIndex};

    /// A draw below `bound` from `state`, by `SplitMix64`.
    fn draw(state: &mut u64, bound: u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % bound
    }

    /// `count` bodies from `seed`: each at a whole centimeter within `span` meters of the origin
    /// on both axes, of a radius from 0.2 to 1.19 m, and that may be pushed, and walks, at
    /// random.
    pub(crate) fn scene(seed: u64, count: usize, span: u64) -> Vec<Collider> {
        let mut state = seed;
        let mut ids = IdAllocator::default();
        let mut world = World::new();
        let centimeters = |cm: i64| Num::from_bits((cm << Num::FRAC_BITS) / 100);
        (0..count)
            .map(|_| {
                let mut coordinate = || {
                    let cm = draw(&mut state, span * 200).cast_signed();
                    centimeters(cm - span.cast_signed() * 100)
                };
                let at = Vec3::new(coordinate(), Num::ZERO, coordinate());
                let radius = centimeters(20 + draw(&mut state, 100).cast_signed());
                let movable = draw(&mut state, 4) != 0;
                Collider {
                    id: ids.allocate(),
                    entity: world.spawn_empty().id(),
                    at,
                    radius,
                    movable,
                    walking: movable && draw(&mut state, 2) == 0,
                }
            })
            .collect()
    }

    /// The static index of the colliders that may not be pushed, for the widest of the others.
    pub(crate) fn statics(colliders: &[Collider]) -> StaticIndex {
        let widest = colliders
            .iter()
            .filter(|collider| collider.movable)
            .map(|collider| collider.radius)
            .max();
        let mut index = StaticIndex::new(widest.unwrap_or(Num::ONE));
        let bodies: Vec<StaticBody> = colliders
            .iter()
            .filter(|collider| !collider.movable)
            .map(|collider| StaticBody {
                id: collider.id,
                at: Position::new(collider.at).unwrap(),
                radius: collider.radius,
            })
            .collect();
        index.update(&bodies);
        index
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Vec3;

    use super::internals::{scene, statics};
    use super::*;

    /// The pairs a check of every pair finds, in its order.
    fn every_pair(colliders: &[Collider]) -> Vec<Contact> {
        let mut contacts = Vec::new();
        for first in 0..colliders.len() {
            for second in first + 1..colliders.len() {
                if colliders[first].overlaps(&colliders[second]) {
                    contacts.push(Contact { first, second });
                }
            }
        }
        contacts
    }

    #[test]
    fn the_cells_find_every_overlap_a_check_of_every_pair_finds() {
        let mut broadphase = Broadphase::default();
        // Crowded, sparse, and spread across cells on both sides of the origin, from several
        // seeds, each with the buffers the scene before left.
        let mut crowded = 0;
        for (seed, count, span) in [
            (1, 300, 8),
            (2, 300, 8),
            (3, 500, 40),
            (4, 60, 3),
            (5, 2, 1),
        ] {
            let colliders = scene(seed, count, span);
            let expected = every_pair(&colliders);
            let index = statics(&colliders);
            assert_eq!(
                broadphase.contacts(&colliders, &index),
                expected,
                "seed {seed}"
            );
            crowded = crowded.max(expected.len());
        }
        assert!(
            crowded > 100,
            "{crowded} contacts in the most crowded scene"
        );
        assert_eq!(broadphase.contacts(&[], &statics(&[])), []);

        // Walkers of 0.35 m over 160 m square, the first body a static one of 64 m at the origin,
        // which about half of them overlap: the cells stay 0.7 m wide.
        let walker = Num::from_bits((35 << Num::FRAC_BITS) / 100);
        for seed in [6, 7] {
            let mut colliders = scene(seed, 400, 80);
            for collider in colliders.iter_mut().filter(|collider| collider.movable) {
                collider.radius = walker;
            }
            colliders[0].at = Vec3::ZERO;
            colliders[0].radius = Num::from_int(64).unwrap();
            colliders[0].movable = false;
            colliders[0].walking = false;
            let expected = every_pair(&colliders);
            let wide = expected.iter().filter(|contact| contact.first == 0).count();
            assert!(wide > 100, "{wide} walkers overlap the wide body");
            let index = statics(&colliders);
            assert_eq!(
                broadphase.contacts(&colliders, &index),
                expected,
                "seed {seed}"
            );
            assert_eq!(Broadphase::cell(&colliders), Some(walker + walker));
        }
    }
}
