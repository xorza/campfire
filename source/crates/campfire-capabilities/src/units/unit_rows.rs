use std::fmt;
use std::mem;

use bevy_ecs::archetype::ArchetypeId;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, ROQueryItem};
use bevy_ecs::system::{Query, SystemState};
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{EntityIndex, Position, SimTick, StableId};

use crate::geometry::bounds::Bounds;
use crate::geometry::metric::Metric;
use crate::units::body::Body;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::row_fill::{FillRow, RowFill, RowSource};
use crate::units::row_marks::RowMarks;
use crate::units::row_parts::RowParts;
use crate::units::source_reads::SourceReads;
use crate::units::spawn_point::SpawnPoint;
use crate::units::team::Team;
use crate::units::unit_row::UnitRow;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::units::view_column::{ViewColumn, ViewColumns};

/// The units scripts see, those with a position and a team, as the running phase of the tick
/// began, and what every row reads besides them: the reader that fills the rows again before
/// each phase that runs scripts, keeping those of units that did not change.
#[derive(Debug)]
pub(crate) struct UnitRows {
    /// The core's parts of each unit, once a read built its queries.
    core: Option<CoreSource>,
    /// How each installed capability above the core fills its fields of a row, in install order.
    sources: Vec<Box<dyn FillRow>>,
    /// The tick the units were read in.
    now: Tick,
    /// By stable id.
    units: Vec<UnitRow>,
    /// The rows of the read before, which a read keeps the rows of unchanged units from.
    kept_units: Vec<UnitRow>,
    /// Each unit of the entity index as the last read found it, by stable id, and the one before.
    seen: Vec<Seen>,
    kept_seen: Vec<Seen>,
    /// The rows each source must fill again in the running read.
    marks: RowMarks,
    /// Each source's part of the running read.
    reads: SourceReads,
    /// Whether the next read must fill every row, as what the rows derive from besides the
    /// units' parts changed.
    refill: bool,
    /// How the teams regard each other, as the units were read.
    relations: Relations,
    metric: Metric,
    bounds: Bounds,
    /// What each capability above the core reads of the units, and its getters read besides.
    columns: ViewColumns,
}

/// The parts of a unit the core reads into its row.
type CoreParts = (
    Option<&'static Position>,
    Option<&'static Team>,
    Option<&'static Body>,
    Option<&'static SpawnPoint>,
    Option<&'static UnitType>,
    Option<&'static Owner>,
    Option<&'static UnitTags>,
);

/// The core's parts of each unit, and the units whose parts changed since the last read.
struct CoreSource {
    parts: QueryState<CoreParts>,
    changed: SystemState<Query<'static, 'static, Entity, <CoreParts as RowParts>::Changed>>,
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

impl UnitRows {
    /// The mark bit of the core's parts; each source's follows, in install order.
    const CORE_MARK: usize = 0;

    /// The mark bit of the source at `at` among the sources.
    const fn source_mark(at: usize) -> usize {
        UnitRows::CORE_MARK + 1 + at
    }

    /// Reads the units of `world`: fills the rows of units whose parts changed since the last
    /// read, or that are new to it, and keeps the others. A debug build reads again, every row
    /// filled, and checks that the two reads agree.
    pub(crate) fn read(&mut self, world: &mut World) {
        self.read_rows(world, false);
        if cfg!(debug_assertions) {
            self.read_rows(world, true);
            assert!(
                self.units == self.kept_units && self.columns.same_as_kept(),
                "a read keeps exactly the rows a full read fills"
            );
        }
    }

    /// Starts a read of `world`: takes what every row reads besides the units, and marks the
    /// rows each source must fill again. Whether every row must be filled.
    fn begin_read(&mut self, world: &World, full: bool) -> bool {
        let now = world.resource::<SimTick>().start();
        let relations = world.resource::<Relations>();
        let refill = mem::take(&mut self.refill) || full || *relations != self.relations;
        self.now = now;
        self.relations.clone_from(relations);
        self.metric = *world.resource::<Metric>();
        self.bounds = *world.resource::<Bounds>();
        let core = self
            .core
            .as_mut()
            .expect("a read builds the core's queries");
        core.parts.update_archetypes(world);
        let changed = core.changed.get(world).expect("a query is always valid");
        for entity in &changed {
            self.marks.mark(entity, UnitRows::CORE_MARK);
        }
        let (columns, marks) = (&mut self.columns, &mut self.marks);
        let sources = self.sources.iter_mut().enumerate();
        self.reads.begin(sources.map(|(at, source)| {
            source.begin(world, columns, UnitRows::source_mark(at), marks) || refill
        }));
        refill
    }

    /// Reads the units of `world`, every row filled when `full`.
    fn read_rows(&mut self, world: &mut World, full: bool) {
        self.core.get_or_insert_with(|| CoreSource::new(world));
        let world: &World = world;
        let refill = self.begin_read(world, full);
        let core = self
            .core
            .as_ref()
            .expect("a read builds the core's queries");
        mem::swap(&mut self.units, &mut self.kept_units);
        mem::swap(&mut self.seen, &mut self.kept_seen);
        self.units.clear();
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
                self.reads.keep_all(&self.sources, &mut self.columns, at);
                self.seen.push(Seen {
                    id,
                    entity,
                    archetype,
                    row: Some(self.units.len()),
                });
                self.units.push(self.kept_units[at]);
                continue;
            }
            self.reads.take_clean(&self.sources, &mut self.columns);
            let kept_row = kept.map(|at| self.kept_units[at]);
            let row = if kept.is_some() && !self.marks.marked(entity, UnitRows::CORE_MARK) {
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
                let mark = UnitRows::source_mark(at);
                match kept {
                    Some(kept) if !read.refills() && !self.marks.marked(entity, mark) => {
                        read.keep(source.as_ref(), &mut self.columns, kept..kept + 1);
                    }
                    _ => {
                        read.flush(source.as_ref(), &mut self.columns);
                        source.fill(world, entity, &mut row, &mut self.columns, &self.relations);
                    }
                }
            }
            self.seen.push(Seen {
                id,
                entity,
                archetype,
                row: Some(self.units.len()),
            });
            self.units.push(row);
        }
        self.reads.finish(&self.sources, &mut self.columns);
        self.marks.clear();
        debug_assert!(
            self.columns.hold(self.units.len()),
            "every column holds a row for each unit"
        );
    }

    /// The rows, by stable id.
    pub(crate) fn units(&self) -> &[UnitRow] {
        &self.units
    }

    /// The place of unit `id` among the rows, when the read found it.
    pub(crate) fn index(&self, id: StableId) -> Option<usize> {
        self.units.binary_search_by_key(&id, |row| row.id).ok()
    }

    /// The row of unit `id`, when the read found it.
    pub(crate) fn row(&self, id: StableId) -> Option<&UnitRow> {
        Some(&self.units[self.index(id)?])
    }

    /// The tick the units were read in.
    pub(crate) const fn now(&self) -> Tick {
        self.now
    }

    /// How the teams regard each other, as the units were read.
    pub(crate) const fn relations(&self) -> &Relations {
        &self.relations
    }

    pub(crate) const fn metric(&self) -> Metric {
        self.metric
    }

    /// The map's bounds, as the units were read.
    pub(crate) const fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub(crate) const fn columns(&self) -> &ViewColumns {
        &self.columns
    }

    /// Adds how a capability fills its column of each row, and its fields of the core row, from
    /// the parts `D` of `world`'s units, after those added before it.
    pub(crate) fn add_source<D: RowParts, C: ViewColumn>(
        &mut self,
        world: &mut World,
        fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>),
    ) {
        assert!(
            UnitRows::source_mark(self.sources.len()) < RowMarks::SOURCES,
            "the marks have room for every source and the core"
        );
        let column = self
            .columns
            .index_of::<C>()
            .expect("a capability adds its column before its source");
        let source = RowSource::<D, C>::new(world, column, fill);
        self.sources.push(Box::new(source));
        self.refill = true;
    }

    /// Adds `column`, which a source fills.
    pub(crate) fn add_column<C: ViewColumn>(&mut self, column: C) {
        self.columns.add(column);
    }

    /// The column of type `C`, to change, when one was added; `refill` makes the next read fill
    /// every row again, as the rows may derive from what changes.
    pub(crate) fn column_mut<C: ViewColumn>(&mut self, refill: bool) -> Option<&mut C> {
        let column = self.columns.get_mut()?;
        self.refill |= refill;
        Some(column)
    }

    /// Makes the next read fill every row.
    pub(crate) const fn refill_next(&mut self) {
        self.refill = true;
    }
}

impl Default for UnitRows {
    fn default() -> UnitRows {
        UnitRows {
            core: None,
            sources: Vec::new(),
            now: Tick::ZERO,
            units: Vec::new(),
            kept_units: Vec::new(),
            seen: Vec::new(),
            kept_seen: Vec::new(),
            marks: RowMarks::default(),
            reads: SourceReads::default(),
            refill: true,
            relations: Relations::default(),
            metric: Metric::default(),
            bounds: Bounds::WORLD,
            columns: ViewColumns::default(),
        }
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
    /// fields of the capabilities above it keeps from `kept`; `None` for an entity with no
    /// position or team, which is no unit scripts see.
    fn row(
        &self,
        world: &World,
        id: StableId,
        entity: Entity,
        kept: Option<UnitRow>,
    ) -> Option<UnitRow> {
        let parts = self
            .parts
            .get_manual(world, entity)
            .expect("the core reads optional parts");
        let (Some(&pos), Some(&team), body, spawn, unit_type, owner, tags) = parts else {
            return None;
        };
        let (alive, targetable) = kept.map_or((true, false), |row| (row.alive, row.targetable));
        Some(UnitRow {
            id,
            pos,
            team,
            shape: Body::shape_of(body),
            spawn: spawn.map(|spawn| spawn.get()),
            alive,
            targetable,
            unit_type: unit_type.copied(),
            owner: owner.map(|owner| owner.slot()),
            tags: tags.copied().unwrap_or_default(),
        })
    }
}
