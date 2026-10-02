use std::cell::RefCell;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use bevy_ecs::world::{EntityRef, World};
use campfire_math::{Num, PlayerSlot, Tick, Ticks, Vec3};
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_sim::{Capability, EntityIndex, Position, SimTick, StableId, TickRate};

use crate::units::action_id::ActionId;

use crate::players::resource_id::ResourceId;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::MemberSpec;
use crate::scripts::script_consts::ScriptConsts;
use crate::scripts::state_value::StateValue;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::living_unit::LivingUnit;
use crate::units::modifier_id::ModifierId;
use crate::units::owner::Owner;
use crate::units::path_id::PathId;
use crate::units::recent_attack::RecentAttack;
use crate::units::relations::Relations;
use crate::units::spawn_point::SpawnPoint;
use crate::units::tag::Tag;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::units::teams::Teams;
use crate::units::track_id::TrackId;
use crate::units::type_scope::TypeScope;
use crate::units::unit::Unit;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::units::view_column::{ViewColumn, ViewColumns};
use crate::values::attitude::Attitude;
use crate::values::bounds::Bounds;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::metric::Metric;
use crate::values::stat::Stat;

/// What scripts see: the match's unit types, and its units, those with a team, as the running
/// phase of the tick began. The units are read again before each phase that runs
/// scripts, and every call of the phase sees them as they were read.
#[derive(Debug)]
pub(crate) struct ScriptView {
    types: UnitTypes,
    /// The match's teams, once a mode sets them.
    teams: Rc<Teams>,
    /// The name of each path, by index, once a mode sets them.
    paths: Arc<[Box<str>]>,
    /// The names scripts read, as they read them.
    consts: ScriptConsts,
    /// How each installed capability above the core fills its fields of a row, in install order.
    sources: Vec<RowSource>,
    rate: TickRate,
    /// The tick the units were read in.
    now: Tick,
    /// By stable id.
    units: Vec<UnitRow>,
    /// How the teams regard each other, as the units were read.
    relations: Relations,
    metric: Metric,
    bounds: Bounds,
    /// The recent attacks on each unit, one run per unit.
    attacks: Vec<RecentAttack>,
    /// The stats the mode declares, in order, and each unit's values of them, one run per unit.
    stat_names: Arc<[Stat]>,
    stats: Vec<Num>,
    /// The pools the mode declares, by pool id.
    pool_names: Arc<[DeclaredName]>,
    /// The players' resources the mode declares, by resource id.
    resource_names: Arc<[DeclaredName]>,
    /// What each capability above the core reads of the units, and its getters read besides.
    columns: ViewColumns,
    /// Every modifier, by id, the modifiers each unit carries, one run per unit, and their
    /// script state, one run per modifier.
    modifier_book: ModifierBook,
    modifiers: Vec<ModifierRow>,
    modifier_state: Vec<StateValue>,
}

/// A unit as the view read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitRow {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) team: Team,
    /// Its body's radius, 0 for a unit with no body.
    pub(crate) radius: Num,
    /// Where it spawned, if it did as a unit of the mode.
    pub(crate) spawn: Option<Position>,
    pub(crate) unit_type: Option<UnitType>,
    /// The player who controls it.
    pub(crate) owner: Option<PlayerSlot>,
    /// Whether it is not dead; `combat` fills it, and the next three.
    pub(crate) alive: bool,
    /// Whether it stays when dead, for the mode to respawn.
    pub(crate) stays: bool,
    /// The path it walks or stands on; `navigation` fills it.
    pub(crate) path: Option<PathId>,
    /// Its level and pools; `stats` fills them, and its run of stats.
    pub(crate) level: Option<u32>,
    pub(crate) pools: Option<Pools>,
    /// The teams that see it; `vision` fills it, and without vision every team does.
    pub(crate) seen_by: TeamSet,
    /// Its tags and their effects, as the core derives them.
    pub(crate) tags: UnitTags,
    /// Its run of recent attacks, from `attacks_start` to `attacks_end`.
    attacks_start: u32,
    attacks_end: u32,
    /// Its run of stats, in the order of the view's stat names, empty for a unit with none.
    stats_start: u32,
    stats_end: u32,
    /// Its run of modifiers, by id, then source.
    modifiers_start: u32,
    modifiers_end: u32,
}

/// A modifier a unit carries, as the view read it: which, from whom, its stacks, and its run of
/// script state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModifierRow {
    pub(crate) id: ModifierId,
    pub(crate) source: Option<StableId>,
    pub(crate) stacks: u32,
    pub(crate) state: Range<u32>,
}

/// Fills the fields of a unit's row that a capability above the core holds.
pub(crate) type RowSource = fn(&EntityRef<'_>, &mut RowFill<'_>);

/// A row the view reads, as a capability fills it: its fields, the view's buffers of recent
/// attacks, to which the row's runs are added, and the columns.
#[derive(Debug)]
pub(crate) struct RowFill<'a> {
    pub(crate) row: &'a mut UnitRow,
    attacks: &'a mut Vec<RecentAttack>,
    stats: &'a mut Vec<Num>,
    modifiers: &'a mut Vec<ModifierRow>,
    modifier_state: &'a mut Vec<StateValue>,
    columns: &'a mut ViewColumns,
}

impl UnitRow {
    pub(crate) fn is_avatar(&self) -> bool {
        self.tags.tags.contains(EngineTag::Avatar.tag())
    }
}

impl RowFill<'_> {
    /// The column of type `C`, which the source's capability added, for it to add the row's
    /// part to.
    pub(crate) fn column<C: ViewColumn>(&mut self) -> &mut C {
        self.columns
            .get_mut()
            .expect("a source fills the column its capability added")
    }

    /// Adds `instance` to the row's run of modifiers, which the caller adds in order.
    pub(crate) fn modified(
        &mut self,
        id: ModifierId,
        source: Option<StableId>,
        stacks: u32,
        state: &[StateValue],
    ) {
        let start = u32::try_from(self.modifier_state.len()).expect("state fits u32");
        self.modifier_state.extend_from_slice(state);
        let end = u32::try_from(self.modifier_state.len()).expect("state fits u32");
        self.modifiers.push(ModifierRow {
            id,
            source,
            stacks,
            state: start..end,
        });
    }

    /// Adds `stats`, in the order of the view's stat names, as the row's run of stats.
    pub(crate) fn stated(&mut self, stats: &[Num]) {
        self.stats.extend_from_slice(stats);
    }

    /// Adds `attacks` to the row's run of recent attacks.
    pub(crate) fn attacked(&mut self, attacks: impl IntoIterator<Item = RecentAttack>) {
        self.attacks.extend(attacks);
    }
}

/// The view as the host and every handle share it.
#[derive(Clone)]
pub(crate) struct View(Rc<RefCell<ScriptView>>);

impl ScriptView {
    fn read(&mut self, world: &World) {
        self.now = world.resource::<SimTick>().start();
        self.relations.clone_from(world.resource::<Relations>());
        self.metric = *world.resource::<Metric>();
        self.bounds = Bounds::of(world);
        self.units.clear();
        self.columns.clear();
        self.attacks.clear();
        self.stats.clear();
        self.modifiers.clear();
        self.modifier_state.clear();
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            let unit = world.entity(entity);
            let (Some(&pos), Some(&team)) = (unit.get::<Position>(), unit.get::<Team>()) else {
                continue;
            };
            let start = u32::try_from(self.attacks.len()).expect("attacks fit u32");
            let stats_start = u32::try_from(self.stats.len()).expect("stats fit u32");
            let modifiers_start = u32::try_from(self.modifiers.len()).expect("modifiers fit u32");
            let mut row = UnitRow {
                id,
                pos,
                team,
                radius: Body::radius_of(unit.get::<Body>()),
                spawn: unit.get::<SpawnPoint>().map(|spawn| spawn.get()),
                alive: true,
                stays: false,
                unit_type: unit.get::<UnitType>().copied(),
                owner: unit.get::<Owner>().map(|owner| owner.slot()),
                path: None,
                level: None,
                pools: None,
                seen_by: TeamSet::ALL,
                tags: unit.get::<UnitTags>().copied().unwrap_or_default(),
                attacks_start: start,
                attacks_end: start,
                stats_start,
                stats_end: stats_start,
                modifiers_start,
                modifiers_end: modifiers_start,
            };
            let mut fill = RowFill {
                row: &mut row,
                attacks: &mut self.attacks,
                stats: &mut self.stats,
                modifiers: &mut self.modifiers,
                modifier_state: &mut self.modifier_state,
                columns: &mut self.columns,
            };
            for source in &self.sources {
                source(&unit, &mut fill);
            }
            row.attacks_end = u32::try_from(self.attacks.len()).expect("attacks fit u32");
            row.stats_end = u32::try_from(self.stats.len()).expect("stats fit u32");
            row.modifiers_end = u32::try_from(self.modifiers.len()).expect("modifiers fit u32");
            self.units.push(row);
        }
        debug_assert!(
            self.columns.hold(self.units.len()),
            "every column holds a row for each unit"
        );
    }

    fn row(&self, id: StableId) -> Option<UnitRow> {
        let index = self.units.binary_search_by_key(&id, |row| row.id).ok()?;
        Some(self.units[index])
    }

    /// The living units that may be targets and that `filter` selects relative to `of`.
    fn selected<'a>(
        &'a self,
        of: &UnitRow,
        filter: &str,
    ) -> Result<impl Iterator<Item = &'a UnitRow>, ApiError> {
        let filter = Filter::parse(filter, &self.types)?;
        let of = of.team;
        Ok(self.units.iter().filter(move |row| {
            let targetable = !row.tags.effects.blocks(Block::Target);
            let attitude = self.relations.between(of, row.team);
            row.alive && targetable && filter.selects(attitude, row.tags.tags)
        }))
    }
}

impl View {
    pub(crate) fn new(rate: TickRate) -> View {
        View(Rc::new(RefCell::new(ScriptView {
            types: UnitTypes::default(),
            teams: Rc::default(),
            paths: Arc::default(),
            consts: ScriptConsts::default(),
            sources: Vec::new(),
            rate,
            now: Tick::ZERO,
            units: Vec::new(),
            relations: Relations::default(),
            metric: Metric::default(),
            bounds: Bounds::WORLD,
            attacks: Vec::new(),
            stat_names: Arc::from([]),
            pool_names: Arc::from([]),
            resource_names: Arc::from([]),
            columns: ViewColumns::default(),
            stats: Vec::new(),
            modifier_book: ModifierBook::default(),
            modifiers: Vec::new(),
            modifier_state: Vec::new(),
        })))
    }

    /// Reads the units of `world` for the phase that begins.
    pub(crate) fn read(&self, world: &World) {
        self.0.borrow_mut().read(world);
    }

    /// The map's bounds, as the units were read.
    pub(crate) fn bounds(&self) -> Bounds {
        self.0.borrow().bounds
    }

    pub(crate) fn metric(&self) -> Metric {
        self.0.borrow().metric
    }

    /// Player `player`'s slot, as a script names it, when the session has it.
    pub(crate) fn player(&self, player: INT) -> Checked<PlayerSlot> {
        self.0.borrow().teams.player(player)
    }

    /// Names the teams and the paths.
    pub(crate) fn set_names(&self, teams: Rc<Teams>, paths: Arc<[Box<str>]>) {
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

    /// `ms` in ticks at the match's rate, rounded up, at least one; an error for a negative time
    /// or one too long to count.
    pub(crate) fn ticks(&self, ms: INT) -> Checked<Ticks> {
        let ms = u64::try_from(ms)
            .ok()
            .ok_or_else(|| ApiError::NegativeTime.fail())?;
        let ticks = self.0.borrow().rate.duration(ms);
        Ok(ticks.ok_or_else(|| ApiError::TimeTooLarge.fail())?)
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

    /// Shares the match's modifiers, as the load built them.
    pub(crate) fn set_modifiers(&self, book: ModifierBook) {
        self.0.borrow_mut().modifier_book = book;
    }

    /// The modifier `name` of `package`; an error when it declares none.
    pub(crate) fn modifier_named(&self, package: u16, name: &str) -> Checked<ModifierId> {
        let found = self.0.borrow().modifier_book.named(package, name);
        Ok(found.ok_or_else(|| ApiError::UnknownModifier.fail())?)
    }

    /// Whether the unit of `row` carries the modifier `name` of `package`.
    pub(crate) fn has_modifier(&self, row: &UnitRow, package: u16, name: &str) -> Checked<bool> {
        let id = self.modifier_named(package, name)?;
        let view = self.0.borrow();
        let run = &view.modifiers[row.modifiers_start as usize..row.modifiers_end as usize];
        Ok(run.iter().any(|modifier| modifier.id == id))
    }

    /// The handle of the instance of `id` from `source` on `carrier` that an application in the
    /// running call adds or applies again, as the call sees it: a new one's one stack and first
    /// state, or a held one's, a stack more when it stacks, up to its limit. An instance the call
    /// took a handle to before, in `handles`, keeps that handle, so the call sees one instance
    /// once; one it removed is new again.
    pub(crate) fn applied_handle(
        &self,
        handles: &mut Vec<ModifierHandle>,
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    ) -> ModifierHandle {
        let view = self.0.borrow();
        let spec = &view.modifier_book.get(id).spec;
        if let Some(handle) = handles.iter().find(|handle| handle.is(carrier, id, source)) {
            let mut data = handle.data();
            if data.removed {
                data.removed = false;
                data.written = false;
                data.stacks = 1;
                data.state.clone_from_slice(&spec.initial);
            } else {
                data.stacks = spec.reapply.stacks(data.stacks, spec.max_stacks);
            }
            return handle.clone();
        }
        let held = view.row(carrier).and_then(|row| {
            let run = &view.modifiers[row.modifiers_start as usize..row.modifiers_end as usize];
            run.iter()
                .find(|modifier| modifier.id == id && modifier.source == source)
        });
        let (stacks, state) = match held {
            Some(held) => {
                let state =
                    &view.modifier_state[held.state.start as usize..held.state.end as usize];
                let stacks = spec.reapply.stacks(held.stacks, spec.max_stacks);
                (stacks, state.to_vec())
            }
            None => (1, spec.initial.to_vec()),
        };
        drop(view);
        let handle = self.held_handle(carrier, id, source, stacks, state);
        handles.push(handle.clone());
        handle
    }

    /// The handle of `carrier`'s instance of `id` from `source`, as a call sees it: `stacks`
    /// and `state`.
    pub(crate) fn held_handle(
        &self,
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
        stacks: u32,
        state: Vec<StateValue>,
    ) -> ModifierHandle {
        let fields = Arc::clone(&self.0.borrow().modifier_book.get(id).spec.fields);
        ModifierHandle::new(carrier, id, source, stacks, state, fields, self.clone())
    }

    /// Sets the stats the mode declares, in the order units' runs of stats hold them.
    pub(crate) fn set_stat_names(&self, names: Arc<[Stat]>) {
        self.0.borrow_mut().stat_names = names;
    }

    /// How `of` regards `other`, as the units were read.
    pub(crate) fn attitude(&self, of: Team, other: Team) -> Attitude {
        self.0.borrow().relations.between(of, other)
    }

    /// The value of stat `name` of `row`; an error for a stat the mode does not declare, or a
    /// unit with no stats.
    pub(crate) fn stat_named(&self, row: &UnitRow, name: &str) -> Checked<Num> {
        let view = self.0.borrow();
        let at = view
            .stat_names
            .binary_search_by(|stat| stat.order_to(name))
            .ok()
            .ok_or_else(|| ApiError::UnknownStat.fail())?;
        let run = &view.stats[row.stats_start as usize..row.stats_end as usize];
        run.get(at)
            .copied()
            .ok_or_else(|| ApiError::NoStats.fail().into())
    }

    /// Sets the pools the mode declares, by pool id.
    pub(crate) fn set_pool_names(&self, names: Arc<[DeclaredName]>) {
        self.0.borrow_mut().pool_names = names;
    }

    /// The pool `name`; an error for one the mode does not declare.
    pub(crate) fn pool_named(&self, name: &str) -> Checked<PoolId> {
        self.pool_id_named(name)
            .ok_or_else(|| ApiError::UnknownPool.fail().into())
    }

    /// The pool `name`; `None` for one the mode does not declare.
    pub(crate) fn pool_id_named(&self, name: &str) -> Option<PoolId> {
        let view = self.0.borrow();
        let at = view
            .pool_names
            .iter()
            .position(|pool| pool.as_str() == name)?;
        let pool = u8::try_from(at).ok().and_then(PoolId::new);
        Some(pool.expect("the load keeps the pools within the limit"))
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

    /// Adds how a capability fills its fields of each row, after those added before it.
    pub(crate) fn add_source(&self, source: RowSource) {
        self.0.borrow_mut().sources.push(source);
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
    pub(crate) fn column_mut<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        if let Some(column) = self.0.borrow_mut().columns.get_mut() {
            write(column);
        }
    }

    /// The place among the rows of unit `id`, when the view read it.
    pub(crate) fn row_index(&self, id: StableId) -> Option<usize> {
        let view = self.0.borrow();
        view.units.binary_search_by_key(&id, |row| row.id).ok()
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
                Dynamic::from(ImmutableString::from(&**name))
            })
    }

    /// The path named `name`.
    pub(crate) fn path_named(&self, name: &str) -> Option<PathId> {
        let view = self.0.borrow();
        let at = view.paths.iter().position(|held| **held == *name)?;
        Some(PathId::new(at))
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
    fn units_where(&self, keep: impl FnMut(&&UnitRow) -> bool) -> Array {
        let view = self.0.borrow();
        view.units
            .iter()
            .filter(keep)
            .map(|row| Dynamic::from(Unit::new(row.id, self.clone())))
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

    /// The handle of unit `id`, when the view read it.
    pub(crate) fn unit(&self, id: StableId) -> Option<Unit> {
        self.row(id).map(|_| Unit::new(id, self.clone()))
    }

    /// Unit `id`, when it is a living unit that may be a target.
    pub(crate) fn living(&self, id: StableId) -> Option<LivingUnit> {
        let row = self
            .row(id)
            .filter(|row| row.alive && !row.tags.effects.blocks(Block::Target))?;
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

    /// The living units within `radius` of `pos` in the map's metric that `filter` selects
    /// relative to `of`, by stable id; with `visible`, only those `of`'s team sees.
    pub(crate) fn find(
        &self,
        of: &Unit,
        pos: Position,
        radius: Num,
        filter: &str,
        visible: bool,
    ) -> Checked<Array> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        let view = self.0.borrow();
        let of = of.row();
        let selected = view.selected(&of, filter).map_err(ApiError::fail)?;
        Ok(selected
            .filter(|row| !visible || row.seen_by.contains(of.team))
            .filter(|row| view.metric.within(pos, row.pos, radius))
            .map(|row| Dynamic::from(Unit::new(row.id, self.clone())))
            .collect())
    }

    /// The nearest living unit within `radius` of `of` in the map's metric that `filter` selects
    /// relative to it and its team sees, by exact distance, the lower stable id on a tie; `()`
    /// when there is none.
    pub(crate) fn nearest_visible(&self, of: &Unit, radius: Num, filter: &str) -> Checked<Dynamic> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        let view = self.0.borrow();
        let of = of.row();
        let nearest = view
            .selected(&of, filter)
            .map_err(ApiError::fail)?
            .filter(|row| row.seen_by.contains(of.team))
            .map(|row| (view.metric.offset(of.pos, row.pos), row.id))
            .filter(|&(offset, _)| Vec3::ZERO.within(offset, radius))
            .min_by_key(|&(offset, id)| (offset.length_squared_bits(), id));
        Ok(nearest.map_or(Dynamic::UNIT, |(_, id)| {
            Dynamic::from(Unit::new(id, self.clone()))
        }))
    }

    /// The living units that struck `unit` within the last `ms` milliseconds, rounded up to
    /// whole ticks, by stable id.
    pub(crate) fn recent_attackers(&self, unit: &Unit, ms: INT) -> Checked<Array> {
        let ms = u64::try_from(ms)
            .ok()
            .ok_or_else(|| ApiError::NegativeTime.fail())?;
        let view = self.0.borrow();
        let window = view.rate.window(ms);
        let row = unit.row();
        let run = &view.attacks[row.attacks_start as usize..row.attacks_end as usize];
        Ok(run
            .iter()
            .filter(|attack| {
                // A strike later than the view's tick, as a rollback can leave, is not recent.
                view.now.since(attack.tick).is_some_and(|age| age <= window)
            })
            .filter(|attack| view.row(attack.source).is_some_and(|source| source.alive))
            .map(|attack| Dynamic::from(Unit::new(attack.source, self.clone())))
            .collect())
    }

    /// `ctx.find`, `ctx.find_visible` and `ctx.nearest_visible`.
    pub(crate) fn register_queries(api: &mut ApiBuilder<'_>) {
        let find = MemberSpec::call(
            "find",
            "(of, pos, radius, filter)",
            "the living units within `radius` of `pos` that `filter` selects for `of`, seen or not, by stable id",
        )
        .name(3, NameKind::Filter);
        let visible = MemberSpec::call(
            "find_visible",
            "(of, pos, radius, filter)",
            "as `find`, of the units `of`'s team sees",
        )
        .name(3, NameKind::Filter)
        .capability(Capability::Vision);
        for (spec, visible) in [(find, false), (visible, true)] {
            api.bind(
                spec,
                move |ctx: &mut Ctx, of: Unit, pos: Position, radius: Num, filter: &str| {
                    ctx.view().find(&of, pos, radius, filter, visible)
                },
            )
            .bind(
                spec,
                move |ctx: &mut Ctx, of: Unit, pos: Position, radius: INT, filter: &str| {
                    ctx.view()
                        .find(&of, pos, ApiError::num(radius)?, filter, visible)
                },
            );
        }
        let nearest = MemberSpec::call(
            "nearest_visible",
            "(of, radius, filter)",
            "the nearest living unit within `radius` of `of` that `filter` selects and `of`'s team sees, `()` with none",
        ).name(2, NameKind::Filter)
        .capability(Capability::Vision);
        api.bind(
            nearest,
            |ctx: &mut Ctx, of: Unit, radius: Num, filter: &str| {
                ctx.view().nearest_visible(&of, radius, filter)
            },
        )
        .bind(
            nearest,
            |ctx: &mut Ctx, of: Unit, radius: INT, filter: &str| {
                ctx.view()
                    .nearest_visible(&of, ApiError::num(radius)?, filter)
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
    use std::cell::RefMut;

    use crate::actions::action_data::CostTarget;
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
            let at = self.0.borrow().stat_names.binary_search(stat).ok()?;
            Some(StatId::new(at))
        }

        /// What a cost named `name` takes from: a pool, or else a player resource; `None` for
        /// a name the mode declares neither as.
        pub(crate) fn cost_target(&self, name: &str) -> Option<CostTarget> {
            let pool = self.pool_id_named(name).map(CostTarget::Pool);
            pool.or_else(|| self.resource_named(name).map(CostTarget::Resource))
        }
    }
}
