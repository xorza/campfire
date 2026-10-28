use std::any::{TypeId, type_name};
use std::{fmt, mem};

use bevy_ecs::change_detection::Mut;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::lifecycle::{Add, Remove, RemoveEvent};
use bevy_ecs::observer::{Observer, On};
use bevy_ecs::query::{Allow, QueryState, ROQueryItem};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Query, ResMut, SystemState};
use bevy_ecs::world::World;
use campfire_declarations::{Cache, MatchFacts};
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
use crate::unit_entity::UnitEntity;
use crate::units_world::UnitsWorld;

/// What reads the units into the script view's rows before each phase that runs scripts: the
/// core's parts of each unit and the source of each column, which fill the rows of units whose
/// parts changed since the last read and keep the others. A read visits only the units it
/// touches: those whose parts changed, as each source's change ticks show, and those that lost a
/// part a source reads, or went, as its observers record; it copies the rows between them in
/// runs, and a read that touches none changes nothing (02-engine-core.md, The view refills what
/// changed). The view calls it through `read`, which the core's runtime gives it.
#[derive(Resource, Debug, Default)]
pub struct ViewReader {
    /// The core's parts of each unit, once a read built its queries.
    core: Option<CoreSource>,
    /// How each installed capability above the core fills its fields of a row, in install order.
    sources: Vec<Box<dyn FillRow>>,
    /// The rows of the read before, which a read keeps the rows of unchanged units from.
    kept_units: Vec<UnitRow>,
    /// The units that lost a part a source reads, or went, since the last read, as the
    /// observers record them.
    removed: Vec<StableId>,
    /// The units the running read visits, by stable id.
    touched: Vec<Touched>,
    /// The rows each source must fill again in the running read.
    marks: RowMarks,
    /// Each source's part of the running read.
    reads: SourceReads,
    /// Whether the next read fills every row, as the rows the view holds are of the units a load
    /// in place despawned.
    refill: bool,
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

/// A unit a read visits: its stable id, its entity when the read knows it, and whether every
/// source fills its row, as it lost a part, which leaves no change tick.
#[derive(Debug, Clone, Copy)]
struct Touched {
    id: StableId,
    entity: Option<Entity>,
    whole: bool,
}

impl Touched {
    /// A unit that lost a part or went, whose entity the read looks up.
    const fn gone(id: StableId) -> Self {
        Self {
            id,
            entity: None,
            whole: true,
        }
    }
}

impl ViewReader {
    /// The mark bit of the core's parts; each source's follows, in install order.
    const CORE_MARK: usize = 0;

    /// The mark bit of the source at `at` among the sources.
    const fn source_mark(at: usize) -> usize {
        Self::CORE_MARK + 1 + at
    }

    /// Reads the units of `world` into its script view's rows, as the view asks before each
    /// phase that runs scripts.
    pub(crate) fn read(world: &mut World) {
        let view = world.non_send::<View>().clone();
        world.resource_scope(|world, mut reader: Mut<'_, Self>| {
            reader.read_into(world, &mut view.rows_mut());
        });
    }

    /// The observer of the removal of a part the core or a source reads, or of a unit's stable
    /// id, as it despawns: the next read visits the unit, every source filling its row.
    fn part_removed(
        removed: On<'_, '_, RemoveEvent>,
        ids: Query<'_, '_, &StableId>,
        mut reader: ResMut<'_, Self>,
    ) {
        if let Ok(&id) = ids.get(removed.entity) {
            reader.removed.push(id);
        }
    }

    /// The observer of a unit that leaves the world as `Disabled` comes, which no change tick
    /// shows, in a match that reads its units for scripts: the next read visits the unit, which no query of the core finds then, and drops its
    /// row (transports.md, A rider leaves the world and keeps its entity).
    pub(crate) fn disabled(
        disabled: On<'_, '_, Add<Disabled>>,
        ids: Query<'_, '_, &StableId, Allow<Disabled>>,
        reader: Option<ResMut<'_, Self>>,
    ) {
        if let (Ok(&id), Some(mut reader)) = (ids.get(disabled.entity), reader) {
            reader.removed.push(id);
        }
    }

    /// The observer of a unit that comes back into the world as `Disabled` goes, whose parts'
    /// change ticks are of its time out of it: the next read visits it, every source filling its
    /// row.
    pub(crate) fn enabled(
        enabled: On<'_, '_, Remove<Disabled>>,
        ids: Query<'_, '_, &StableId, Allow<Disabled>>,
        reader: Option<ResMut<'_, Self>>,
    ) {
        if let (Ok(&id), Some(mut reader)) = (ids.get(enabled.entity), reader) {
            reader.removed.push(id);
        }
    }

    /// Observes the removal of each part of `components` in `world`, as the reader keeps no
    /// change tick of one.
    fn observe_removals(world: &mut World, components: &[(TypeId, &'static str)]) {
        for &(type_id, name) in components {
            let id = world
                .components()
                .get_id(type_id)
                .unwrap_or_else(|| panic!("a read part is registered: {name}"));
            world.spawn(Observer::new(Self::part_removed).with_component(id));
        }
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
            metric: map.metric(),
            bounds: map.bounds,
        };
        let refill = rows.begin(read) || full || mem::take(&mut self.refill);
        let core = self
            .core
            .as_mut()
            .expect("a read builds the core's queries");
        core.parts.update_archetypes(world);
        let changed = core.changed.get(world).expect("a query is always valid");
        for entity in &changed {
            self.marks.mark(entity, Self::CORE_MARK);
        }
        let (columns, marks) = (rows.columns_mut(), &mut self.marks);
        let sources = self.sources.iter_mut().enumerate();
        self.reads.begin(sources.map(|(at, source)| {
            source.begin(world, columns, Self::source_mark(at), marks) || refill
        }));
        refill
    }

    /// Gathers the units the running read visits, by stable id: those marked and those removed,
    /// or every unit of the entity index when `every`, or when more than half the rows are
    /// marked, where a walk of the index in id order costs less than a sort of the marked and a
    /// lookup of each one's entity.
    fn touch(&mut self, world: &World, every: bool) {
        self.touched.clear();
        self.removed.sort_unstable();
        self.removed.dedup();
        let index = world.resource::<EntityIndex>();
        if every || self.marks.entities().len() > self.kept_units.len() / 2 {
            let mut removed = self.removed.iter().copied().peekable();
            for (id, entity) in index.iter() {
                while let Some(gone) = removed.next_if(|&gone| gone < id) {
                    self.touched.push(Touched::gone(gone));
                }
                self.touched.push(Touched {
                    id,
                    entity: Some(entity),
                    whole: every || removed.next_if_eq(&id).is_some(),
                });
            }
            self.touched.extend(removed.map(Touched::gone));
            if every {
                // A full read visits each kept row too, so one a read left of a unit gone goes.
                let kept = self.kept_units.iter().map(|row| Touched::gone(row.id));
                self.touched.extend(kept);
                self.settle_touched();
            }
        } else {
            for &entity in self.marks.entities() {
                if let Some(&id) = world.get::<StableId>(entity) {
                    self.touched.push(Touched {
                        id,
                        entity: Some(entity),
                        whole: false,
                    });
                }
            }
            self.touched
                .extend(self.removed.iter().copied().map(Touched::gone));
            self.settle_touched();
        }
        self.removed.clear();
    }

    /// Sorts the visits by stable id, each unit once, whole when any of its visits is, with the
    /// entity any of them knows.
    fn settle_touched(&mut self) {
        self.touched.sort_unstable_by_key(|touched| touched.id);
        self.touched.dedup_by(|later, kept| {
            let same = later.id == kept.id;
            if same {
                kept.whole |= later.whole;
                kept.entity = kept.entity.or(later.entity);
            }
            same
        });
    }

    /// Reads the units of `world` into `rows`, every row filled when `full`.
    fn read_rows(&mut self, world: &mut World, rows: &mut ViewRows, full: bool) {
        self.core.get_or_insert_with(|| CoreSource::new(world));
        let world: &World = world;
        let refill = self.begin_read(world, rows, full);
        let any_refills = self.reads.any_refills();
        self.touch(world, refill || any_refills);
        if self.touched.is_empty() {
            self.marks.clear();
            return;
        }
        for source in &self.sources {
            source.start_rows(rows.columns_mut());
        }
        let core = self
            .core
            .as_ref()
            .expect("a read builds the core's queries");
        mem::swap(rows.units_mut(), &mut self.kept_units);
        rows.units_mut().clear();
        let index = world.resource::<EntityIndex>();
        let mut next = 0;
        for &Touched { id, entity, whole } in &self.touched {
            let end = next + self.kept_units[next..].partition_point(|row| row.id < id);
            if next < end {
                rows.units_mut()
                    .extend_from_slice(&self.kept_units[next..end]);
                self.reads
                    .keep_all(&self.sources, rows.columns_mut(), next..end);
            }
            next = end;
            let found = self.kept_units.get(next).is_some_and(|row| row.id == id);
            let kept = found.then_some(next).filter(|_| !refill && !whole);
            next += usize::from(found);
            let Some(entity) = entity.or_else(|| index.get(id)) else {
                continue;
            };
            self.reads.take_clean(&self.sources, rows.columns_mut());
            let kept_row = kept.map(|at| self.kept_units[at]);
            let row = if kept.is_some() && !self.marks.is_marked(entity, Self::CORE_MARK) {
                kept_row
            } else {
                core.row(world, id, entity, kept_row)
            };
            let Some(mut row) = row else {
                continue;
            };
            let sources = self.sources.iter().zip(self.reads.each()).enumerate();
            for (at, (source, read)) in sources {
                let mark = Self::source_mark(at);
                match kept {
                    Some(kept) if !read.refills() && !self.marks.is_marked(entity, mark) => {
                        read.keep(source.as_ref(), rows.columns_mut(), kept..kept + 1);
                    }
                    _ => {
                        read.flush(source.as_ref(), rows.columns_mut());
                        source.fill(world, entity, &mut row, rows.columns_mut());
                    }
                }
            }
            rows.units_mut().push(row);
        }
        let end = self.kept_units.len();
        if next < end {
            rows.units_mut()
                .extend_from_slice(&self.kept_units[next..end]);
            self.reads
                .keep_all(&self.sources, rows.columns_mut(), next..end);
        }
        self.reads.end(&self.sources, rows.columns_mut());
        self.marks.clear();
        debug_assert!(
            rows.columns().hold(rows.units().len()),
            "every column holds a row for each unit"
        );
    }

    /// Adds how a capability fills its column `C` of each row of `world`'s view, and its fields
    /// of the core row, from the parts `D` of the units, after those added before it: the column
    /// first, then its source, and the observers of the removal of its parts. The next read fills
    /// every row.
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
        world.resource_scope(|world, mut reader: Mut<'_, Self>| {
            assert!(
                Self::source_mark(reader.sources.len()) < RowMarks::SOURCES,
                "the marks have room for every source and the core"
            );
            let source = RowSource::<D, C>::new(world, column, fill);
            reader.sources.push(Box::new(source));
        });
        let mut components = Vec::new();
        D::components(&mut components);
        Self::observe_removals(world, &components);
        rows.refill_next();
    }
}

/// A match that runs scripts reads its view's rows, from none kept.
impl Cache for ViewReader {
    fn installed(world: &World) -> Option<Self> {
        let facts = world.resource::<MatchFacts>();
        facts.role.scripts().then(Self::default)
    }

    fn clear(&mut self) {
        self.removed.clear();
        self.marks.clear();
        self.refill = true;
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
    /// The core's queries of `world`, and the observers of the removal of its parts and of each
    /// unit's stable id.
    fn new(world: &mut World) -> Self {
        let core = Self {
            parts: QueryState::new(world),
            changed: SystemState::new(world),
        };
        let mut components = vec![
            (TypeId::of::<StableId>(), type_name::<StableId>()),
            (TypeId::of::<UnitEntity>(), type_name::<UnitEntity>()),
        ];
        <CoreReads as RowReads>::components(&mut components);
        ViewReader::observe_removals(world, &components);
        core
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
        let (&position, &team, &unit_type, body, spawn, owner, tags) =
            self.parts.get_manual(world, entity).ok()?;
        let (alive, targetable) = kept.map_or((true, false), |row| (row.alive, row.targetable));
        Some(UnitRow {
            id,
            position: position.get(),
            team,
            shape: Body::shape_of(body),
            spawn: spawn.map(|spawn| spawn.get()),
            alive,
            targetable,
            unit_type,
            owner: owner.map(|owner| owner.player_slot()),
            tags: tags.copied().unwrap_or_default(),
        })
    }
}
