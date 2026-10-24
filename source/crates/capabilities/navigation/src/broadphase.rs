use campfire_geometry::{CellKey, CellLevels};

use crate::body_index::BodyIndex;
use crate::collider::Collider;
use crate::collider::contact::{Contact, ContactBodies};

/// Finds the pairs of bodies of one layer within each other's personal space, `Collider::near`'s.
/// Two walkers come from a sort of the walkers by layer, level and cell, in levels by size from a
/// base cell twice the narrowest walker's space: a walker goes into the least level whose cell is
/// twice its space, so two of one level that are near sit in the same cell or in cells side by
/// side, and each occupied cell pairs its own
/// walkers, and those of the cell to its right and of the three below it, so each pair of cells is
/// visited once; cursors that only move forward find those cells. A walker and one of a coarser
/// level that are near lie less than the coarser cell apart, so each occupied cell pairs its walkers
/// with those of the nine cells around its own at each coarser level of its layer; a wide walker so
/// costs its own level's few cells, not every walker's. A sort, not a grid over the map, as a map
/// may be wide and its bodies few; each walker's cell is one `CellKey` above its index, so the sort
/// and every search compare one number, as `BodyGrid`'s do. A walker and a static body come from
/// the static index, so a wide structure does not make the cells wide, two static bodies never
/// part, and the colliders hold the walkers alone. The buffers stay between ticks, so a tick
/// allocates nothing once they have grown, and costs `n log n` and the pairs of bodies in cells
/// side by side.
#[derive(Debug, Default)]
pub(crate) struct Broadphase {
    /// Each walker's cell above its index, sorted by layer, level and cell, row by row, then by
    /// index.
    entries: Vec<Entry>,
    /// The occupied cells, in the order of `entries`, each with its run of entries.
    cells: Vec<Cell>,
    contacts: Vec<Contact>,
}

/// A walker's cell, the high half, above its index among the colliders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Entry(u128);

/// An occupied cell, and the run of `entries` in it.
#[derive(Debug, Clone, Copy)]
struct Cell {
    key: CellKey,
    start: usize,
    end: usize,
}

impl Entry {
    fn new(cell: CellKey, index: usize) -> Self {
        let index = u64::try_from(index).expect("colliders fit u64");
        Self((u128::from(cell.to_bits()) << 64) | u128::from(index))
    }

    const fn cell(self) -> CellKey {
        CellKey::from_bits((self.0 >> 64) as u64)
    }

    const fn index(self) -> usize {
        self.0 as u64 as usize
    }
}

impl Broadphase {
    /// The pairs of the walkers `colliders`, sorted by stable id, and of a walker and a static body
    /// of `statics`, that are near as they stand now, by the stable ids of the two, the lower
    /// first: the pairs a check of every pair finds, in its order.
    pub(crate) fn contacts(&mut self, colliders: &[Collider], statics: &BodyIndex) -> &[Contact] {
        debug_assert!(colliders.iter().all(|collider| collider.movable));
        self.entries.clear();
        self.cells.clear();
        self.contacts.clear();
        let Some(levels) = Self::levels(colliders) else {
            return &self.contacts;
        };
        let walkers = colliders.iter().enumerate();
        self.entries
            .extend(walkers.clone().map(|(index, collider)| {
                let level = levels.level(collider.space());
                let cell = CellKey::new(
                    collider.layer.get(),
                    level,
                    levels.index(level, collider.point.z.to_bits()),
                    levels.index(level, collider.point.x.to_bits()),
                );
                Entry::new(cell, index)
            }));
        self.entries.sort_unstable();
        for (at, entry) in self.entries.iter().enumerate() {
            match self.cells.last_mut() {
                Some(cell) if cell.key == entry.cell() => cell.end = at + 1,
                _ => self.cells.push(Cell {
                    key: entry.cell(),
                    start: at,
                    end: at + 1,
                }),
            }
        }
        let (mut below, mut level_end) = (0, 0);
        for (at, &cell) in self.cells.iter().enumerate() {
            let run = &self.entries[cell.start..cell.end];
            for (offset, a) in run.iter().enumerate() {
                for b in &run[offset + 1..] {
                    Self::check(colliders, a.index(), b.index(), &mut self.contacts);
                }
            }
            let key = cell.key;
            let right = self
                .cells
                .get(at + 1)
                .filter(|next| next.key == key.beside(0, 1));
            let (first, last) = (key.beside(1, -1), key.beside(1, 1));
            while self.cells.get(below).is_some_and(|other| other.key < first) {
                below += 1;
            }
            let under = self.cells[below..]
                .iter()
                .take_while(|other| other.key <= last);
            for other in right.into_iter().chain(under) {
                Self::pair(
                    colliders,
                    run,
                    &self.entries[other.start..other.end],
                    &mut self.contacts,
                );
            }
            if at == level_end {
                level_end = at + Self::level_len(&self.cells[at..]);
            }
            let coarser = &self.cells[level_end..];
            Self::across(colliders, coarser, &self.entries, cell, &mut self.contacts);
        }
        for (walker, collider) in walkers {
            statics.near(collider.layer, collider.point, collider.space(), |&body| {
                if collider.near(&Collider::fixed(&body)) {
                    let (first, second) = if collider.id < body.id {
                        (collider.id, body.id)
                    } else {
                        (body.id, collider.id)
                    };
                    self.contacts.push(Contact {
                        first,
                        second,
                        bodies: ContactBodies::Static { walker, body },
                    });
                }
            });
        }
        self.contacts
            .sort_unstable_by_key(|contact| (contact.first, contact.second));
        &self.contacts
    }

    /// The levels of the cells: from a base twice the narrowest space of the walkers; `None` with
    /// none, as no two static bodies part.
    fn levels(colliders: &[Collider]) -> Option<CellLevels> {
        let narrowest = colliders.iter().map(Collider::space).min()?;
        Some(CellLevels::new(narrowest + narrowest))
    }

    /// Checks the walkers of `cell` against those of the nine cells around its own at each coarser
    /// level of its layer, whose cells lead `coarser`, level by level.
    fn across(
        colliders: &[Collider],
        coarser: &[Cell],
        entries: &[Entry],
        cell: Cell,
        contacts: &mut Vec<Contact>,
    ) {
        let (key, run) = (cell.key, &entries[cell.start..cell.end]);
        let mut level = coarser;
        while let Some(first) = level.first().map(|other| other.key)
            && first.layer() == key.layer()
        {
            let around = key.within(first.level());
            for rows in -1..=1 {
                let (from, to) = (around.beside(rows, -1), around.beside(rows, 1));
                let start = level.partition_point(|other| other.key < from);
                let near = level[start..].iter().take_while(|other| other.key <= to);
                for other in near {
                    Self::pair(colliders, run, &entries[other.start..other.end], contacts);
                }
            }
            level = &level[Self::level_len(level)..];
        }
    }

    /// How many of `cells` lead it in the layer and level of its first.
    fn level_len(cells: &[Cell]) -> usize {
        let Some(first) = cells.first().map(|cell| cell.key) else {
            return 0;
        };
        cells.partition_point(|cell| cell.key.same_level(first))
    }

    /// Checks each walker of `run` against each of `others`.
    fn pair(colliders: &[Collider], run: &[Entry], others: &[Entry], contacts: &mut Vec<Contact>) {
        for a in run {
            for b in others {
                Self::check(colliders, a.index(), b.index(), contacts);
            }
        }
    }

    /// Records the contact of the walkers `a` and `b` when they are near.
    fn check(colliders: &[Collider], a: usize, b: usize, contacts: &mut Vec<Contact>) {
        if colliders[a].near(&colliders[b]) {
            let (first, second) = (a.min(b), a.max(b));
            contacts.push(Contact {
                first: colliders[first].id,
                second: colliders[second].id,
                bodies: ContactBodies::Walkers { first, second },
            });
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_geometry::internals::KernelScene;
    use campfire_math::Num;
    use campfire_sim::IdAllocator;
    use campfire_units::Layer;

    use crate::body_index::{BodyIndex, IndexedBody};
    use crate::collider::Collider;

    /// `count` bodies from `seed`: each at a whole centimeter within `span` meters of the origin
    /// on both axes, of a radius from 0.2 to 1.19 m, on one of `layers` layers, and that may be
    /// pushed, and walks, at random. A scene of one layer draws no layer. With `boxes`, a body
    /// that may not be pushed is a box as wide and as deep as its circle, at a whole degree.
    pub fn scene(seed: u64, count: usize, span: u64, layers: u8, boxes: bool) -> Vec<Collider> {
        let mut ids = IdAllocator::default();
        let mut world = World::new();
        let bodies = KernelScene::new(seed).bodies(count, span, layers, boxes);
        bodies
            .into_iter()
            .map(|body| Collider {
                id: ids.allocate(),
                entity: world.spawn_empty().id(),
                point: body.point,
                shape: body.shape,
                layer: Layer::new(body.layer),
                movable: body.movable,
                walking: body.walking,
                gathering: false,
                weight: Num::ONE,
                way: None,
                team: None,
                firm: false,
                leaving: false,
                pinned: false,
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
        index.update(&static_bodies(colliders));
        index
    }

    /// The colliders that may not be pushed, as the static index holds them, by stable id.
    pub(crate) fn static_bodies(colliders: &[Collider]) -> Vec<IndexedBody> {
        colliders
            .iter()
            .filter(|collider| !collider.movable)
            .map(indexed)
            .collect()
    }

    /// The body of `collider` as an index holds it.
    pub(crate) const fn indexed(collider: &Collider) -> IndexedBody {
        IndexedBody {
            id: collider.id,
            position: collider.point,
            shape: collider.shape,
            layer: collider.layer,
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_geometry::{Radius, Shape};
    use campfire_math::{Num, Vec3};
    use campfire_sim::StableId;
    use campfire_units::Layer;

    use super::internals::{scene, statics};
    use super::*;

    /// The pairs of `bodies`, sorted by stable id, a check of every pair finds, in its order, by
    /// the stable ids of the two.
    fn every_pair(bodies: &[Collider]) -> Vec<[StableId; 2]> {
        let mut contacts = Vec::new();
        for first in 0..bodies.len() {
            for second in first + 1..bodies.len() {
                if bodies[first].near(&bodies[second]) {
                    contacts.push([bodies[first].id, bodies[second].id]);
                }
            }
        }
        contacts
    }

    /// The radius of the personal space of a walker of `radius`, 5/4 of it.
    fn space(radius: Num) -> Num {
        Num::from_bits(radius.to_bits() * 5 / 4)
    }

    /// The contacts the broadphase finds among `bodies`, its walkers among the colliders and the
    /// others in the static index, by the stable ids of the two.
    fn found(broadphase: &mut Broadphase, bodies: &[Collider]) -> Vec<[StableId; 2]> {
        let walkers: Vec<Collider> = bodies.iter().filter(|body| body.movable).copied().collect();
        let index = statics(bodies);
        let contacts = broadphase.contacts(&walkers, &index);
        contacts
            .iter()
            .map(|contact| [contact.first, contact.second])
            .collect()
    }

    #[test]
    fn the_cells_find_every_near_pair_a_check_of_every_pair_finds() {
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
            assert_eq!(found(&mut broadphase, &colliders), expected, "seed {seed}");
            crowded = crowded.max(expected.len());
        }
        assert!(
            crowded > 100,
            "{crowded} contacts in the most crowded scene"
        );
        assert_eq!(found(&mut broadphase, &[]), Vec::<[StableId; 2]>::new());

        // Walkers of 0.35 m to 2,048 m mixed, over two layers, from a base of twice the narrowest
        // space, 5/4 of 0.35 m, 0.875 m: each size takes its own level, twice its space over the
        // base rounded up to a power of two, the same as twice its radius over 0.7 m, 0.8 m
        // 2.3 to 4, level 2, 3 m 8.6 to 16, level 4, 17 m 48.6 to 64, level 6, 120 m 343 to 512,
        // level 9, and 2,048 m 5,851 to 8,192, level 13; and the pairs within a level and across
        // levels are a check of every pair's.
        let sizes = [35, 80, 300, 1_700, 12_000, 204_800].map(|cm| Num::int(cm) / 100);
        for seed in [12, 13] {
            let mut colliders = scene(seed, 300, 60, 2, true);
            let walkers = colliders.iter_mut().filter(|collider| collider.movable);
            for (at, collider) in walkers.enumerate() {
                let size = match at % 50 {
                    0 => 5,
                    1 | 2 => 4,
                    3..=6 => 3,
                    7..=14 => 2,
                    _ => at % 2,
                };
                collider.shape = Shape::Circle(Radius::new(sizes[size]).unwrap());
            }
            assert_eq!(
                found(&mut broadphase, &colliders),
                every_pair(&colliders),
                "seed {seed}"
            );
            let walkers: Vec<Collider> = colliders.iter().filter(|c| c.movable).copied().collect();
            let levels = Broadphase::levels(&walkers).unwrap();
            assert_eq!(levels, CellLevels::new(space(sizes[0]) * 2));
            assert_eq!(
                sizes.map(|size| levels.level(space(size))),
                [0, 2, 4, 6, 9, 13]
            );
            let mut held: Vec<u8> = broadphase
                .cells
                .iter()
                .map(|cell| cell.key.level())
                .collect();
            held.sort_unstable();
            held.dedup();
            assert_eq!(held, [0, 2, 4, 6, 9, 13]);
        }

        // Walkers of 0.35 m over 160 m square, the first body a static one of 64 m at the origin,
        // which about half of them come near: the cells stay twice a walker's space wide.
        let walker = Num::from_bits((35 << Num::FRAC_BITS) / 100);
        for seed in [6, 7] {
            let mut colliders = scene(seed, 400, 80, 1, false);
            for collider in colliders.iter_mut().filter(|collider| collider.movable) {
                collider.shape = Shape::Circle(Radius::new(walker).unwrap());
            }
            colliders[0].point = Vec3::ZERO;
            colliders[0].shape = Shape::Circle(Radius::new(Num::int(64)).unwrap());
            colliders[0].movable = false;
            colliders[0].walking = false;
            let expected = every_pair(&colliders);
            let wide = expected
                .iter()
                .filter(|contact| contact[0] == colliders[0].id)
                .count();
            assert!(wide > 100, "{wide} walkers overlap the wide body");
            assert_eq!(found(&mut broadphase, &colliders), expected, "seed {seed}");
            let walkers: Vec<Collider> = colliders.iter().filter(|c| c.movable).copied().collect();
            assert_eq!(
                Broadphase::levels(&walkers),
                Some(CellLevels::new(space(walker) * 2))
            );
        }
    }
}
