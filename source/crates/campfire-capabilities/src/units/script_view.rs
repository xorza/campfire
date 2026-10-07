use std::cell::RefCell;
use std::fmt;
use std::mem;
use std::rc::Rc;
use std::sync::Arc;

use bevy_ecs::archetype::ArchetypeId;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, ROQueryItem};
use bevy_ecs::system::{Query, SystemState};
use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_math::Num;
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_sim::{EntityIndex, Position, SimTick, StableId, TickRate};

use crate::players::resource_id::ResourceId;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_consts::ScriptConsts;
use crate::units::action_id::ActionId;
use crate::units::body::Body;
use crate::units::body_grid::{BodyGrid, Placed};
use crate::units::filter::Filter;
use crate::units::living_unit::LivingUnit;
use crate::units::owner::Owner;
use crate::units::path_id::PathId;
use crate::units::relations::Relations;
use crate::units::row_fill::{FillRow, RowFill, RowSource};
use crate::units::row_marks::RowMarks;
use crate::units::row_parts::RowParts;
use crate::units::source_reads::SourceReads;
use crate::units::spawn_point::SpawnPoint;
use crate::units::tag::Tag;
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::track_id::TrackId;
use crate::units::type_scope::TypeScope;
use crate::units::unit::Unit;
use crate::units::unit_row::UnitRow;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::units::view_column::{ViewColumn, ViewColumns};
use crate::values::attitude::Attitude;
use crate::values::bounds::Bounds;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::metric::Metric;
use crate::values::name_list::NameList;

/// What scripts see: the match's unit types, and its units, those with a team, as the running
/// phase of the tick began. The units are read again before each phase that runs
/// scripts, and every call of the phase sees them as they were read.
#[derive(Debug)]
pub(crate) struct ScriptView {
    types: UnitTypes,
    /// The match's teams, once a mode sets them.
    teams: Rc<Teams>,
    /// The name of each path, by index, once a mode sets them.
    paths: Arc<NameList>,
    /// The names scripts read, as they read them.
    consts: ScriptConsts,
    /// The core's parts of each unit, once a read built its queries.
    core: Option<CoreSource>,
    /// How each installed capability above the core fills its fields of a row, in install order.
    sources: Vec<Box<dyn FillRow>>,
    rate: TickRate,
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
    /// The players' resources the mode declares, by resource id.
    resource_names: Arc<[DeclaredName]>,
    /// The bodies of the units that may be targets, by row, once a query that reaches by
    /// distance asks for them after a read; and the rows such a query found.
    bodies: BodyGrid<usize>,
    indexed: bool,
    found: RefCell<Vec<usize>>,
    /// What each capability above the core reads of the units, and its getters read besides.
    columns: ViewColumns,
}

/// The view as the host and every handle share it.
#[derive(Clone)]
pub(crate) struct View(Rc<RefCell<ScriptView>>);

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
            radius: Body::radius_of(body),
            spawn: spawn.map(|spawn| spawn.get()),
            alive,
            targetable,
            unit_type: unit_type.copied(),
            owner: owner.map(|owner| owner.slot()),
            tags: tags.copied().unwrap_or_default(),
        })
    }
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

impl ScriptView {
    /// Reads the units of `world`: fills the rows of units whose parts changed since the last
    /// read, or that are new to it, and keeps the others. A debug build reads again, every row
    /// filled, and checks that the two reads agree.
    fn read(&mut self, world: &mut World) {
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
        self.bounds = Bounds::of(world);
        self.indexed = false;
        let core = self
            .core
            .as_mut()
            .expect("a read builds the core's queries");
        core.parts.update_archetypes(world);
        let changed = core.changed.get(world).expect("a query is always valid");
        for entity in &changed {
            self.marks.mark(entity, 0);
        }
        let (columns, marks) = (&mut self.columns, &mut self.marks);
        let sources = self.sources.iter_mut().enumerate();
        self.reads.begin(
            sources.map(|(at, source)| source.begin(world, columns, at + 1, marks) || refill),
        );
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
            let row = if kept.is_some() && !self.marks.marked(entity, 0) {
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
                match kept {
                    Some(kept) if !read.refills() && !self.marks.marked(entity, at + 1) => {
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

    fn row(&self, id: StableId) -> Option<UnitRow> {
        Some(self.units[self.index(id)?])
    }

    /// The place of unit `id` among the rows, when the view read it.
    fn index(&self, id: StableId) -> Option<usize> {
        self.units.binary_search_by_key(&id, |row| row.id).ok()
    }

    /// Indexes the bodies of the units that may be targets, once after each read.
    fn index_bodies(&mut self) {
        if self.indexed {
            return;
        }
        let rows = self.units.iter().enumerate();
        let targets = rows.filter(|(_, row)| row.targetable);
        self.bodies.rebuild(targets.map(|(at, row)| Placed {
            id: row.id,
            key: at,
            at: row.pos,
            radius: row.radius,
        }));
        self.indexed = true;
    }
}

impl View {
    pub(crate) fn new(rate: TickRate) -> View {
        View(Rc::new(RefCell::new(ScriptView {
            types: UnitTypes::default(),
            teams: Rc::default(),
            paths: Arc::default(),
            consts: ScriptConsts::default(),
            core: None,
            sources: Vec::new(),
            rate,
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
            resource_names: Arc::from([]),
            bodies: BodyGrid::default(),
            indexed: false,
            found: RefCell::default(),
            columns: ViewColumns::default(),
        })))
    }

    /// Reads the units of `world` for the phase that begins.
    pub(crate) fn read(&self, world: &mut World) {
        self.0.borrow_mut().read(world);
    }

    /// The map's bounds, as the units were read.
    pub(crate) fn bounds(&self) -> Bounds {
        self.0.borrow().bounds
    }

    pub(crate) fn metric(&self) -> Metric {
        self.0.borrow().metric
    }

    /// Whether the match loaded `unit_type`.
    pub(crate) fn has_type(&self, unit_type: UnitType) -> bool {
        self.0.borrow().types.contains(unit_type)
    }

    /// Whether `kind` is one of the mode's damage kinds; any is, before a mode names them.
    pub(crate) fn has_damage_kind(&self, kind: DamageKind) -> bool {
        self.0.borrow().consts.has_damage_kind(kind)
    }

    /// Whether `team` is one of the mode's teams; any team is, in a match no mode set the teams
    /// of, as every mode has one at the least.
    pub(crate) fn has_team(&self, team: Team) -> bool {
        let teams = &self.0.borrow().teams;
        teams.count() == 0 || usize::from(team.index()) < teams.count()
    }

    /// Whether `slot` is a player of the session; any slot is, in a match no mode set the teams
    /// of.
    pub(crate) fn has_player(&self, slot: PlayerSlot) -> bool {
        let teams = &self.0.borrow().teams;
        teams.count() == 0 || teams.of(slot).is_some()
    }

    /// Whether amounts of `resources` resources in `amounts` places make a row of each resource
    /// the mode declares for each player; any do, in a match no mode set the teams of.
    pub(crate) fn fits_resources(&self, resources: usize, amounts: usize) -> bool {
        let view = self.0.borrow();
        let players = view.teams.players() as usize;
        view.teams.count() == 0
            || (resources == view.resource_names.len()
                && Some(amounts) == players.checked_mul(resources))
    }

    /// Player `player`'s slot, as a script names it, when the session has it.
    pub(crate) fn player(&self, player: INT) -> Checked<PlayerSlot> {
        self.0.borrow().teams.player(player)
    }

    /// Names the teams and the paths.
    pub(crate) fn set_names(&self, teams: Rc<Teams>, paths: Arc<NameList>) {
        let mut view = self.0.borrow_mut();
        view.teams = teams;
        view.paths = paths;
    }

    /// Sets the damage kinds and the players' resources the mode declares, by id.
    pub(crate) fn set_mode_names(
        &self,
        damage_kinds: &[DeclaredName],
        resources: Arc<[DeclaredName]>,
    ) {
        let mut view = self.0.borrow_mut();
        let kinds = damage_kinds.iter().map(DeclaredName::as_str);
        view.consts.set_damage_kinds(kinds);
        view.resource_names = resources;
    }

    /// The tick the units were read in.
    pub(crate) fn now(&self) -> Tick {
        self.0.borrow().now
    }

    /// The match's tick rate.
    pub(crate) fn rate(&self) -> TickRate {
        self.0.borrow().rate
    }

    /// `ms` in ticks at the match's rate, rounded up, at least one, or all ticks for a time too
    /// long to count: a window back from now.
    pub(crate) fn window(&self, ms: u64) -> Ticks {
        self.0.borrow().rate.window(ms)
    }

    /// `ms` in ticks at the match's rate, rounded up, at least one; an error for a negative time
    /// or one too long to count.
    pub(crate) fn ticks(&self, ms: INT) -> Checked<Ticks> {
        Ok(self.duration(ms).map_err(ApiError::fail)?)
    }

    /// `ms`, more than 0, in ticks as `ticks` gives them: what lasts, as a reveal or a knock
    /// back, lasts some time.
    pub(crate) fn lasting(&self, ms: INT) -> Result<Ticks, ApiError> {
        if ms == 0 {
            return Err(ApiError::ZeroTime);
        }
        self.duration(ms)
    }

    fn duration(&self, ms: INT) -> Result<Ticks, ApiError> {
        let ms = u64::try_from(ms).ok().ok_or(ApiError::NegativeTime)?;
        let ticks = self.0.borrow().rate.duration(ms);
        ticks.ok_or(ApiError::TimeTooLarge)
    }

    /// Sets the match's unit types and tags, as the load built them.
    pub(crate) fn set_types(&self, types: UnitTypes) {
        self.0.borrow_mut().types = types;
        self.share_type_names();
    }

    /// Gives scripts the names of the unit types as the view holds them.
    pub(crate) fn share_type_names(&self) {
        let view = &mut *self.0.borrow_mut();
        view.consts.set_unit_types(view.types.names());
    }

    /// How `of` regards `other`, as the units were read.
    pub(crate) fn attitude(&self, of: Team, other: Team) -> Attitude {
        self.0.borrow().relations.between(of, other)
    }

    /// The player resource `name`; `None` for one the mode does not declare.
    pub(crate) fn resource_named(&self, name: &str) -> Option<ResourceId> {
        ResourceId::named(&self.0.borrow().resource_names, name)
    }

    /// The damage kind `name`; an error for one the mode does not declare.
    pub(crate) fn damage_kind_named(&self, name: &str) -> Checked<DamageKind> {
        let found = self.0.borrow().consts.damage_kind_named(name);
        Ok(found.ok_or_else(|| ApiError::UnknownDamageKind.fail())?)
    }

    /// Names the tracks the mode declares, by track id.
    pub(crate) fn set_track_names<'a>(&self, names: impl Iterator<Item = &'a str>) {
        self.0.borrow_mut().consts.set_tracks(names);
    }

    /// The name of track `track`.
    pub(crate) fn track_name(&self, track: TrackId) -> ImmutableString {
        self.0.borrow().consts.track(track)
    }

    /// The name of damage kind `kind`.
    pub(crate) fn damage_kind_name(&self, kind: DamageKind) -> ImmutableString {
        self.0.borrow().consts.damage_kind(kind)
    }

    /// Names the match's actions, by action id.
    pub(crate) fn set_action_names<'a>(&self, names: impl Iterator<Item = &'a str>) {
        self.0.borrow_mut().consts.set_actions(names);
    }

    /// The name of ability `id` in its package.
    pub(crate) fn ability_name(&self, id: ActionId) -> ImmutableString {
        self.0.borrow().consts.action(id)
    }

    /// Adds how a capability fills its column of each row, and its fields of the core row,
    /// from the parts `D` of `world`'s units, after those added before it. Its column comes
    /// first.
    pub(crate) fn add_source<D: RowParts, C: ViewColumn>(
        &self,
        world: &mut World,
        fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>),
    ) {
        let mut view = self.0.borrow_mut();
        assert!(
            view.sources.len() < RowMarks::SOURCES - 1,
            "the marks have room for every source and the core"
        );
        let column = view
            .columns
            .index_of::<C>()
            .expect("a capability adds its column before its source");
        let source = RowSource::<D, C>::new(world, column, fill);
        view.sources.push(Box::new(source));
        view.refill = true;
    }

    /// Adds `column`, which a source fills.
    pub(crate) fn add_column<C: ViewColumn>(&self, column: C) {
        self.0.borrow_mut().columns.add(column);
    }

    /// What `read` gives of the column of type `C`; `None` when none was added.
    pub(crate) fn column<C: ViewColumn, R>(&self, read: impl FnOnce(&C) -> R) -> Option<R> {
        self.0.borrow().columns.get().map(read)
    }

    /// Changes the column of type `C` by `write`, when one was added.
    /// The next read fills every row again, as the rows may derive from what `write` changes.
    pub(crate) fn column_mut<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        let view = &mut *self.0.borrow_mut();
        if let Some(column) = view.columns.get_mut() {
            write(column);
            view.refill = true;
        }
    }

    /// Makes the next read fill every row. Bevy clamps each change tick older than it compares
    /// exactly as it checks the world's ticks, but not the last read of the view's queries of
    /// changed parts, so after a check those queries may miss a change.
    pub(crate) fn refill_next(&self) {
        self.0.borrow_mut().refill = true;
    }

    /// Changes the rows of the column of type `C` by `write`, as a call's effect changes the
    /// units' parts too, which the next read finds changed.
    pub(crate) fn write_rows<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        if let Some(column) = self.0.borrow_mut().columns.get_mut() {
            write(column);
        }
    }

    /// The place among the rows of unit `id`, when the view read it.
    pub(crate) fn row_index(&self, id: StableId) -> Option<usize> {
        self.0.borrow().index(id)
    }

    /// The name of `team`.
    pub(crate) fn team_name(&self, team: Team) -> Checked<Dynamic> {
        let view = self.0.borrow();
        let name = view.teams.name(team);
        name.map(|name| Dynamic::from(ImmutableString::from(name)))
            .ok_or_else(|| ApiError::UnknownTeam.fail().into())
    }

    /// The name of `path`, `()` for none.
    pub(crate) fn path_name(&self, path: Option<PathId>) -> Dynamic {
        let view = self.0.borrow();
        path.and_then(|path| view.paths.get(path.index()))
            .map_or(Dynamic::UNIT, |name| {
                Dynamic::from(ImmutableString::from(name))
            })
    }

    /// The path named `name`.
    pub(crate) fn path_named(&self, name: &str) -> Option<PathId> {
        let view = self.0.borrow();
        view.paths.named(name).map(PathId::new)
    }

    /// The unit type named `name` in the mode's scope: one of the mode's, or an avatar.
    pub(crate) fn unit_type_named(&self, name: &str) -> Option<UnitType> {
        self.0.borrow().types.named(TypeScope::Mode, name)
    }

    /// The name of the unit type of `row`, `()` for a unit of no type.
    pub(crate) fn unit_type_name(&self, row: &UnitRow) -> Dynamic {
        let view = self.0.borrow();
        row.unit_type.map_or(Dynamic::UNIT, |unit_type| {
            Dynamic::from(view.consts.unit_type(unit_type))
        })
    }

    /// The tag `name`; one the match does not have fails the call.
    pub(crate) fn tag_named(&self, name: &str) -> Result<Tag, ApiError> {
        self.0
            .borrow()
            .types
            .tag_named(name)
            .ok_or(ApiError::UnknownTag)
    }

    /// Every unit, living or dead, that `keep` keeps, by stable id.
    fn units_where(&self, mut keep: impl FnMut(&UnitRow) -> bool) -> Array {
        let view = self.0.borrow();
        view.units
            .iter()
            .enumerate()
            .filter(|(_, row)| keep(row))
            .map(|(at, row)| Dynamic::from(Unit::new(row.id, at, self.clone())))
            .collect()
    }

    /// Every unit, living or dead, with the tag `name`, by stable id.
    pub(crate) fn units_tagged(&self, name: &str) -> Checked<Array> {
        let tag = self.tag_named(name).map_err(ApiError::fail)?;
        Ok(self.units_where(|row| row.tags.tags.contains(tag)))
    }

    /// Every avatar, living or dead, of `team` or of every team, by stable id.
    pub(crate) fn avatars(&self, team: Option<Team>) -> Array {
        self.units_where(|row| row.is_avatar() && team.is_none_or(|team| row.team == team))
    }

    pub(crate) fn row(&self, id: StableId) -> Option<UnitRow> {
        self.0.borrow().row(id)
    }

    /// The row at `at` among the rows, as a handle names it.
    pub(crate) fn row_at(&self, at: usize) -> UnitRow {
        self.0.borrow().units[at]
    }

    /// The handle of unit `id`, when the view read it.
    pub(crate) fn unit(&self, id: StableId) -> Option<Unit> {
        let at = self.row_index(id)?;
        Some(Unit::new(id, at, self.clone()))
    }

    /// Unit `id`, when it is a living unit that may be a target.
    pub(crate) fn living(&self, id: StableId) -> Option<LivingUnit> {
        let row = self.row(id).filter(|row| row.targetable)?;
        Some(LivingUnit {
            id,
            pos: row.pos,
            team: row.team,
            radius: row.radius,
            tags: row.tags.tags,
        })
    }

    /// The param `name` of the unit type of `row`.
    pub(crate) fn param_named(&self, row: &UnitRow, name: &str) -> Option<Dynamic> {
        let view = self.0.borrow();
        let value = view.types.param_named(row.unit_type?, name)?;
        Some(value.to_dynamic())
    }

    /// The living targets whose bodies come within `radius` of `pos` in the map's metric, as an
    /// area of that radius reaches, that `filter` selects relative to `of`, and `seen` lets by
    /// the view's columns and their rows, by stable id.
    pub(crate) fn find(
        &self,
        of: &Unit,
        pos: Position,
        radius: Num,
        filter: &str,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> Checked<Array> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        self.0.borrow_mut().index_bodies();
        let view = self.0.borrow();
        let of = of.row().team;
        let filter = Filter::parse(filter, &view.types).map_err(ApiError::fail)?;
        let mut found = view.found.borrow_mut();
        found.clear();
        view.bodies.visit_near(pos, radius, |body| {
            let row = &view.units[body.key];
            let attitude = view.relations.between(of, row.team);
            let reaches = view
                .metric
                .reaches(pos, Num::ZERO, radius, body.at, body.radius);
            if reaches && filter.selects(attitude, row.tags.tags) && seen(&view.columns, body.key) {
                found.push(body.key);
            }
        });
        found.sort_unstable();
        let unit = |&at: &usize| Dynamic::from(Unit::new(view.units[at].id, at, self.clone()));
        Ok(found.iter().map(unit).collect())
    }

    /// The nearest living target that `radius` from the edge of `of`'s body reaches in the map's
    /// metric, as a weapon's range does, that `filter` selects relative to it and `seen` lets by
    /// the view's columns and its row, by exact distance between centres, the lower stable id on
    /// a tie; `()` when there is none.
    pub(crate) fn nearest(
        &self,
        of: &Unit,
        radius: Num,
        filter: &str,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> Checked<Dynamic> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        self.0.borrow_mut().index_bodies();
        let view = self.0.borrow();
        let of = of.row();
        let filter = Filter::parse(filter, &view.types).map_err(ApiError::fail)?;
        let reach = of.radius.checked_add(radius).unwrap_or(Num::MAX);
        let mut nearest = None;
        view.bodies.visit_near(of.pos, reach, |body| {
            let row = &view.units[body.key];
            let attitude = view.relations.between(of.team, row.team);
            let reaches = view
                .metric
                .reaches(of.pos, of.radius, radius, body.at, body.radius);
            if !(reaches
                && filter.selects(attitude, row.tags.tags)
                && seen(&view.columns, body.key))
            {
                return;
            }
            let distance = view.metric.offset(of.pos, body.at).length_squared_bits();
            let candidate = (distance, body.id, body.key);
            if nearest.is_none_or(|best| candidate < best) {
                nearest = Some(candidate);
            }
        });
        Ok(nearest.map_or(Dynamic::UNIT, |(_, id, at)| {
            Dynamic::from(Unit::new(id, at, self.clone()))
        }))
    }

    /// `ctx.find`.
    pub(crate) fn register_queries(api: &mut ApiBuilder<'_>) {
        let find = MemberSpec::call(
            "find",
            "(of, pos, radius, filter)",
            "the living targets whose bodies come within `radius` of `pos`, as an area's, that `filter` selects for `of`, seen or not, by stable id",
        )
        .name(3, NameKind::Filter);
        api.bind(
            find,
            |ctx: &mut Ctx, of: Unit, pos: Position, radius: Num, filter: &str| {
                ctx.view().find(&of, pos, radius, filter, |_, _| true)
            },
        )
        .bind(
            find,
            |ctx: &mut Ctx, of: Unit, pos: Position, radius: INT, filter: &str| {
                let radius = ApiError::num(radius)?;
                ctx.view().find(&of, pos, radius, filter, |_, _| true)
            },
        );
    }
}

/// The view's units change every phase; a handle kept in a failure prints only that it is one.
impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("View")
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::stats::stats_column::StatsColumn;
    use std::cell::RefMut;

    use crate::actions::cost_target::CostTarget;
    use crate::scripts::error::ApiError;
    use crate::stats::stat_id::StatId;
    use crate::units::filter::Filter;
    use crate::units::script_view::View;
    use crate::units::unit_types::UnitTypes;
    use crate::values::filter_data::FilterData;
    use crate::values::stat::Stat;

    impl View {
        /// The run-time form of `filter`, its tag among those of the match's unit types.
        pub(crate) fn resolve_filter(&self, filter: &FilterData) -> Result<Filter, ApiError> {
            Filter::resolve(filter, &self.0.borrow().types)
        }

        /// How many unit types the match loaded, for a test to name the next one.
        pub(crate) fn types_count(&self) -> usize {
            self.0.borrow().types.count()
        }

        pub(crate) fn types_mut(&self) -> RefMut<'_, UnitTypes> {
            RefMut::map(self.0.borrow_mut(), |view| &mut view.types)
        }

        /// The place of `stat` among the stats the mode declares; `None` when it does not
        /// declare it.
        pub(crate) fn stat_index(&self, stat: &Stat) -> Option<StatId> {
            self.column(|column: &StatsColumn| column.stat_index(stat))
                .flatten()
        }

        /// What a cost named `name` takes from: a pool, or else a player resource; `None` for
        /// a name the mode declares neither as.
        pub(crate) fn cost_target(&self, name: &str) -> Option<CostTarget> {
            let pool = StatsColumn::pool_id_named(self, name).map(CostTarget::Pool);
            pool.or_else(|| self.resource_named(name).map(CostTarget::Resource))
        }
    }
}
