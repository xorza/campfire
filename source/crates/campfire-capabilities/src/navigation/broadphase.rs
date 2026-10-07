use campfire_math::Num;

use crate::navigation::body_index::BodyIndex;
use crate::navigation::collider::Collider;
use crate::units::layer::Layer;

/// Finds the pairs of bodies of one layer that overlap. Two walkers come from a sort of the walkers
/// by layer and cell: a cell is twice the widest walker's radius, so two that overlap sit in the
/// same cell of their layer or in cells side by side. Each occupied cell pairs its own walkers, and
/// those of the cell to its right and of the three below it, so each pair of cells is visited once;
/// cursors that only move forward find those cells. A sort, not a grid over the map, as a map may
/// be wide and its bodies few. A walker and a static body come from the static index, so a wide
/// structure does not make the cells wide, and two static bodies never part. The buffers stay
/// between ticks, so a tick allocates nothing once they have grown, and costs `n log n` and the
/// pairs of bodies in cells side by side.
#[derive(Debug, Default)]
pub(crate) struct Broadphase {
    /// Each collider's layer, cell and index, sorted by layer, then cell, row by row, then by
    /// index.
    entries: Vec<Entry>,
    /// The occupied cells, in the order of `entries`, each with its run of entries.
    cells: Vec<Cell>,
    contacts: Vec<Contact>,
}

/// A collider's layer, its cell and its index among the colliders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    layer: Layer,
    row: i64,
    column: i64,
    index: usize,
}

/// An occupied cell of a layer, and the run of `entries` in it.
#[derive(Debug, Clone, Copy)]
struct Cell {
    layer: Layer,
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
    pub(crate) fn contacts(&mut self, colliders: &[Collider], statics: &BodyIndex) -> &[Contact] {
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
                layer: collider.layer,
                row: collider.at.z.to_bits().div_euclid(size),
                column: collider.at.x.to_bits().div_euclid(size),
                index,
            }));
        self.entries.sort_unstable();
        for (at, entry) in self.entries.iter().enumerate() {
            match self.cells.last_mut() {
                Some(cell)
                    if (cell.layer, cell.row, cell.column)
                        == (entry.layer, entry.row, entry.column) =>
                {
                    cell.end = at + 1;
                }
                _ => self.cells.push(Cell {
                    layer: entry.layer,
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
            let key = |cell: &Cell| (cell.layer, cell.row, cell.column);
            let right = self
                .cells
                .get(at + 1)
                .filter(|next| key(next) == (cell.layer, cell.row, cell.column + 1));
            while self
                .cells
                .get(below)
                .is_some_and(|other| key(other) < (cell.layer, cell.row + 1, cell.column - 1))
            {
                below += 1;
            }
            let under = self.cells[below..]
                .iter()
                .take_while(|other| key(other) <= (cell.layer, cell.row + 1, cell.column + 1));
            for other in right.into_iter().chain(under) {
                for a in run {
                    for b in &self.entries[other.start..other.end] {
                        Broadphase::check(colliders, a.index, b.index, &mut self.contacts);
                    }
                }
            }
        }
        for (index, collider) in walkers {
            statics.near(collider.layer, collider.at, collider.radius(), |body| {
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
            .map(Collider::radius)
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
    use campfire_math::Num;
    use campfire_sim::{IdAllocator, Position};

    use crate::navigation::body_index::{BodyIndex, IndexedBody};
    use crate::navigation::collider::Collider;
    use crate::units::layer::Layer;
    use crate::values::body_box::BodyBox;
    use crate::values::kernel_scene::KernelScene;
    use crate::values::shape::Shape;

    /// `count` bodies from `seed`: each at a whole centimeter within `span` meters of the origin
    /// on both axes, of a radius from 0.2 to 1.19 m, on one of `layers` layers, and that may be
    /// pushed, and walks, at random. A scene of one layer draws no layer. With `boxes`, a body
    /// that may not be pushed is a box as wide and as deep as its circle, at a whole degree.
    pub(crate) fn scene(
        seed: u64,
        count: usize,
        span: u64,
        layers: u8,
        boxes: bool,
    ) -> Vec<Collider> {
        let mut scene = KernelScene::new(seed);
        let mut ids = IdAllocator::default();
        let mut world = World::new();
        (0..count)
            .map(|_| {
                let at = scene.point(span);
                let radius = KernelScene::centimeters(20 + scene.below(100).cast_signed());
                let movable = scene.below(4) != 0;
                let layer = match layers {
                    1 => Layer::FIRST,
                    _ => Layer::new(u8::try_from(scene.below(layers.into())).unwrap()),
                };
                let shape = if boxes && !movable {
                    let angle = Num::from_int(scene.below(360).cast_signed()).unwrap();
                    let side = radius + radius;
                    Shape::Box(BodyBox::new([side, side], angle).unwrap())
                } else {
                    Shape::Circle(radius)
                };
                Collider {
                    id: ids.allocate(),
                    entity: world.spawn_empty().id(),
                    at,
                    shape,
                    layer,
                    movable,
                    walking: movable && scene.below(2) == 0,
                    gathering: false,
                }
            })
            .collect()
    }

    /// The static index of the colliders that may not be pushed, for the widest of the others.
    pub(crate) fn statics(colliders: &[Collider]) -> BodyIndex {
        let widest = colliders
            .iter()
            .filter(|collider| collider.movable)
            .map(Collider::radius)
            .max();
        let mut index = BodyIndex::new(widest.unwrap_or(Num::ONE));
        let bodies: Vec<IndexedBody> = colliders
            .iter()
            .filter(|collider| !collider.movable)
            .map(|collider| IndexedBody {
                id: collider.id,
                at: Position::new(collider.at).unwrap(),
                shape: collider.shape,
                layer: collider.layer,
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
    use crate::values::shape::Shape;

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
        // seeds, each with the buffers the scene before left; the last two over two and three
        // layers, where bodies of other layers that overlap on the ground plane have no contact.
        let mut crowded = 0;
        // Two crowded scenes hold boxes for the bodies that may not be pushed.
        for (seed, count, span, layers, boxes) in [
            (1, 300, 8, 1, false),
            (2, 300, 8, 1, false),
            (3, 500, 40, 1, false),
            (4, 60, 3, 1, false),
            (5, 2, 1, 1, false),
            (8, 300, 8, 2, false),
            (9, 300, 8, 3, false),
            (10, 300, 8, 1, true),
            (11, 300, 8, 2, true),
        ] {
            let colliders = scene(seed, count, span, layers, boxes);
            let expected = every_pair(&colliders);
            let mut flat = colliders.clone();
            for collider in &mut flat {
                collider.layer = Layer::FIRST;
            }
            let across = every_pair(&flat).len() - expected.len();
            assert_eq!(across > 0, layers > 1, "{across} contacts across layers");
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
            let mut colliders = scene(seed, 400, 80, 1, false);
            for collider in colliders.iter_mut().filter(|collider| collider.movable) {
                collider.shape = Shape::Circle(walker);
            }
            colliders[0].at = Vec3::ZERO;
            colliders[0].shape = Shape::Circle(Num::from_int(64).unwrap());
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
