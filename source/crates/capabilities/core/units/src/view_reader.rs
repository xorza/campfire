use std::{fmt, mem};

use bevy_ecs::archetype::ArchetypeId;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, ROQueryItem};
use bevy_ecs::system::{Query, SystemState};
use bevy_ecs::world::World;
use campfire_script::{ReadOf, RowFill, RowReads, UnitRow, View, ViewColumn, ViewRows};
use campfire_sim::{EntityIndex, Position, SimTick, StableId};
use campfire_values::{Team, UnitTags, UnitType};

use crate::body::Body;
use crate::entity_filters::UnitFilter;
use crate::map_book::MapBook;
use crate::owner::Owner;
use crate::row_marks::RowMarks;
use crate::row_source::{FillRow, RowSource};
use crate::source_reads::SourceReads;
use crate::spawn_point::SpawnPoint;
use crate::units_world::UnitsWorld;

/// What reads the units into the script view's rows before each phase that runs scripts: the
/// core's parts of each unit and the source of each column, which fill the rows of units whose
/// parts changed since the last read and keep the others. The view calls it through `read`, which
/// `Units::open` gives it.
#[derive(Debug, Default)]
pub struct ViewReader {
    /// The core's parts of each unit, once a read built its queries.
    core: Option<CoreSource>,
    /// How each installed capability above the core fills its fields of a row, in install order.
    sources: Vec<Box<dyn FillRow>>,
    /// The rows of the read before, which a read keeps the rows of unchanged units from.
    kept_units: Vec<UnitRow>,
    /// Each unit of the entity index as the last read found it, by stable id, and the one before.
    seen: Vec<Seen>,
    kept_seen: Vec<Seen>,
    /// The rows each source must fill again in the running read.
    marks: RowMarks,
    /// Each source's part of the running read.
    reads: SourceReads,
}

/// The parts of a unit the core reads into its row: those every unit spawns with, then the
/// others.
type CoreReads = (
    &'static Position,
    &'static Team,
    &'static UnitType,
    Option<&'static Body>,
    Option<&'static SpawnPoint>,
    Option<&'static Owner>,
    Option<&'static UnitTags>,
);

/// The core's parts of each unit, and the units whose parts changed since the last read.
struct CoreSource {
    parts: QueryState<CoreReads, UnitFilter>,
    changed: SystemState<
        Query<'static, 'static, Entity, (UnitFilter, <CoreReads as RowReads>::Changed)>,
    >,
}

/// An entity of the entity index as a read found it: its archetype, which holds the parts it
/// has, and its row, `None` when it is no unit scripts see.
#[derive(Debug, Clone, Copy)]
struct Seen {
    id: StableId,
    entity: Entity,
    archetype: ArchetypeId,
    row: Option<usize>,
}

impl ViewReader {
    /// The mark bit of the core's parts; each source's follows, in install order.
    const CORE_MARK: usize = 0;

    /// The mark bit of the source at `at` among the sources.
    const fn source_mark(at: usize) -> usize {
        ViewReader::CORE_MARK + 1 + at
    }

    /// Reads the units of `world` into its script view's rows, as the view asks before each
    /// phase that runs scripts.
    pub(crate) fn read(world: &mut World) {
        let view = world.non_send::<View>().clone();
        let mut reader = world
            .remove_non_send::<ViewReader>()
            .expect("the core reads the view");
        reader.read_into(world, &mut view.rows_mut());
        world.insert_non_send(reader);
    }

    /// Reads the units of `world` into `rows`: fills the rows of units whose parts changed since
    /// the last read, or that are new to it, and keeps the others. A debug build reads again,
    /// every row filled, and checks that the two reads agree.
    fn read_into(&mut self, world: &mut World, rows: &mut ViewRows) {
        self.read_rows(world, rows, false);
        if cfg!(debug_assertions) {
            self.read_rows(world, rows, true);
            assert!(
                rows.units() == self.kept_units && rows.columns().same_as_kept(),
                "a read keeps exactly the rows a full read fills"
            );
        }
    }

    /// Starts a read of `world` into `rows`: takes what every row reads besides the units, and
    /// marks the rows each source must fill again. Whether every row must be filled.
    fn begin_read(&mut self, world: &World, rows: &mut ViewRows, full: bool) -> bool {
        let map = world.resource::<MapBook>();
        let read = ReadOf {
            now: world.resource::<SimTick>().start(),
            relations: world.relations(),
            metric: map.metric,
            bounds: map.bounds,
        };
        let refill = rows.begin(read) || full;
        let core = self
            .core
            .as_mut()
            .expect("a read builds the core's queries");
        core.parts.update_archetypes(world);
        let changed = core.changed.get(world).expect("a query is always valid");
        for entity in &changed {
            self.marks.mark(entity, ViewReader::CORE_MARK);
        }
        let (columns, marks) = (rows.columns_mut(), &mut self.marks);
        let sources = self.sources.iter_mut().enumerate();
        self.reads.begin(sources.map(|(at, source)| {
            source.begin(world, columns, ViewReader::source_mark(at), marks) || refill
        }));
        refill
    }

    /// Reads the units of `world` into `rows`, every row filled when `full`.
    fn read_rows(&mut self, world: &mut World, rows: &mut ViewRows, full: bool) {
        self.core.get_or_insert_with(|| CoreSource::new(world));
        let world: &World = world;
        let refill = self.begin_read(world, rows, full);
        let core = self
            .core
            .as_ref()
            .expect("a read builds the core's queries");
        mem::swap(rows.units_mut(), &mut self.kept_units);
        mem::swap(&mut self.seen, &mut self.kept_seen);
        rows.units_mut().clear();
        self.seen.clear();
        let any_refills = self.reads.any_refills();
        let mut before = self.kept_seen.iter().peekable();
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            while before.next_if(|seen| seen.id < id).is_some() {}
            let archetype = world
                .entities()
                .get_spawned(entity)
                .expect("the index holds spawned entities")
                .archetype_id;
            let unchanged = before
                .next_if(|seen| seen.id == id)
                .filter(|seen| !refill && seen.entity == entity && seen.archetype == archetype);
            let kept = match unchanged {
                Some(&seen @ Seen { row: None, .. }) => {
                    self.seen.push(seen);
                    continue;
                }
                Some(&Seen { row, .. }) => row,
                None => None,
            };
            if let Some(at) = kept
                && !any_refills
                && !self.marks.any(entity)
            {
                self.reads.keep_all(&self.sources, rows.columns_mut(), at);
                self.seen.push(Seen {
                    id,
                    entity,
                    archetype,
                    row: Some(rows.units().len()),
                });
                rows.units_mut().push(self.kept_units[at]);
                continue;
            }
            self.reads.take_clean(&self.sources, rows.columns_mut());
            let kept_row = kept.map(|at| self.kept_units[at]);
            let row = if kept.is_some() && !self.marks.marked(entity, ViewReader::CORE_MARK) {
                kept_row
            } else {
                core.row(world, id, entity, kept_row)
            };
            let Some(mut row) = row else {
                self.seen.push(Seen {
                    id,
                    entity,
                    archetype,
                    row: None,
                });
                continue;
            };
            let sources = self.sources.iter().zip(self.reads.each()).enumerate();
            for (at, (source, read)) in sources {
                let mark = ViewReader::source_mark(at);
                match kept {
                    Some(kept) if !read.refills() && !self.marks.marked(entity, mark) => {
                        read.keep(source.as_ref(), rows.columns_mut(), kept..kept + 1);
                    }
                    _ => {
                        read.flush(source.as_ref(), rows.columns_mut());
                        source.fill(world, entity, &mut row, rows.columns_mut());
                    }
                }
            }
            self.seen.push(Seen {
                id,
                entity,
                archetype,
                row: Some(rows.units().len()),
            });
            rows.units_mut().push(row);
        }
        self.reads.finish(&self.sources, rows.columns_mut());
        self.marks.clear();
        debug_assert!(
            rows.columns().hold(rows.units().len()),
            "every column holds a row for each unit"
        );
    }

    /// Adds how a capability fills its column `C` of each row of `world`'s view, and its fields
    /// of the core row, from the parts `D` of the units, after those added before it: the column
    /// first, then its source. The next read fills every row.
    pub fn add_source<D: RowReads, C: ViewColumn + Default>(
        world: &mut World,
        fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>),
    ) {
        let view = world.non_send::<View>().clone();
        let mut rows = view.rows_mut();
        rows.add_column(C::default());
        let column = rows
            .columns()
            .index_of::<C>()
            .expect("the column was added");
        let mut reader = world
            .remove_non_send::<ViewReader>()
            .expect("the core reads the view");
        assert!(
            ViewReader::source_mark(reader.sources.len()) < RowMarks::SOURCES,
            "the marks have room for every source and the core"
        );
        reader
            .sources
            .push(Box::new(RowSource::<D, C>::new(world, column, fill)));
        world.insert_non_send(reader);
        rows.refill_next();
    }
}

/// A query prints only what it reads.
impl fmt::Debug for CoreSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CoreSource")
            .field("parts", &self.parts)
            .finish_non_exhaustive()
    }
}

impl CoreSource {
    fn new(world: &mut World) -> CoreSource {
        CoreSource {
            parts: QueryState::new(world),
            changed: SystemState::new(world),
        }
    }

    /// The row of unit `id`, `entity` of `world`, whose fields of the core it fills, and whose
    /// fields of the capabilities above it keeps from `kept`; `None` for an entity that is no
    /// unit: a player's or the match's.
    fn row(
        &self,
        world: &World,
        id: StableId,
        entity: Entity,
        kept: Option<UnitRow>,
    ) -> Option<UnitRow> {
        let (&pos, &team, &unit_type, body, spawn, owner, tags) =
            self.parts.get_manual(world, entity).ok()?;
        let (alive, targetable) = kept.map_or((true, false), |row| (row.alive, row.targetable));
        Some(UnitRow {
            id,
            pos,
            team,
            shape: Body::shape_of(body),
            spawn: spawn.map(|spawn| spawn.get()),
            alive,
            targetable,
            unit_type,
            owner: owner.map(|owner| owner.slot()),
            tags: tags.copied().unwrap_or_default(),
        })
    }
}
