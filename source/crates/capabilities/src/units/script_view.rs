use std::cell::{RefCell, RefMut};
use std::fmt;
use std::num::NonZeroU32;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use bevy_ecs::world::{EntityRef, World};
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_sim::{Capability, EntityIndex, Position, SimTick, StableId, Tick, TickRate, Ticks};

use crate::actions::action_book::{Action, ActionId, Delivery};
use crate::actions::action_data::CostTarget;
use crate::combat::damage_kind::DamageKind;
use crate::mode::resource_id::ResourceId;
use crate::progression::track_id::TrackId;
use crate::progression::track_set::TrackSet;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::MemberSpec;
use crate::scripts::state_value::StateValue;
use crate::stats::modifier_book::ModifierId;
use crate::stats::modifier_data::Reapply;
use crate::stats::modifier_handle::{ModifierHandle, StateField};
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat::Stat;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::living_unit::LivingUnit;
use crate::units::owner::Owner;
use crate::units::path_id::PathId;
use crate::units::recent_attack::RecentAttack;
use crate::units::relations::Relations;
use crate::units::spawn_point::SpawnPoint;
use crate::units::tag::Tag;
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::units::teams::Teams;
use crate::units::type_scope::TypeScope;
use crate::units::unit::Unit;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::values::attitude::Attitude;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::metric::Metric;

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
    /// The damage kinds the mode declares.
    damage_kinds: Rc<[DeclaredName]>,
    /// Each loaded ability's name in its package, by ability id.
    ability_names: Vec<ImmutableString>,
    /// How each loaded ability delivers, if other than at once, by ability id.
    delivers: Vec<Option<Delivery>>,
    /// The unit type each loaded ability spawns, once bound, by ability id: a train's unit, or
    /// its delivery's.
    spawns: Vec<Option<UnitType>>,
    /// Whether each unit type is a projectile type that homes, by unit type; a type past its
    /// end does not.
    homing: Vec<bool>,
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
    /// The recent attacks on each unit, one run per unit.
    attacks: Vec<RecentAttack>,
    /// The ability slots of each unit, one run per unit.
    slots: Vec<SlotRow>,
    /// The stats the mode declares, in order, and each unit's values of them, one run per unit.
    stat_names: Rc<[Stat]>,
    stats: Vec<Num>,
    /// The pools the mode declares, by pool id.
    pool_names: Rc<[DeclaredName]>,
    /// The players' resources the mode declares, by resource id.
    resource_names: Rc<[DeclaredName]>,
    /// The tracks the mode declares, by track id.
    track_names: Rc<[DeclaredName]>,
    /// Every modifier, by id, the modifiers each unit carries, one run per unit, and their
    /// script state, one run per modifier.
    modifier_info: Vec<ModifierInfo>,
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
    pub(crate) target: Option<StableId>,
    pub(crate) attack_range: Option<Num>,
    /// The path it walks or stands on; `navigation` fills it.
    pub(crate) path: Option<PathId>,
    /// Its level and pools; `stats` fills them, and its run of stats.
    pub(crate) level: Option<u32>,
    pub(crate) pools: Option<Pools>,
    /// The teams that see it; `vision` fills it, and without vision every team does.
    pub(crate) seen_by: TeamSet,
    /// The tracks it has; `progression` fills it.
    pub(crate) tracks: TrackSet,
    /// Its tags and their effects, as the core derives them.
    pub(crate) tags: UnitTags,
    /// Its run of recent attacks, from `attacks_start` to `attacks_end`.
    attacks_start: u32,
    attacks_end: u32,
    /// Its run of ability slots, from `slots_start` to `slots_end`; `abilities` fills it.
    slots_start: u32,
    slots_end: u32,
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

/// A modifier as scripts name it: its package and name, its state's fields and their first
/// values, and how a second application from one source acts, up to how many stacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModifierInfo {
    pub(crate) package: u16,
    pub(crate) name: Box<str>,
    pub(crate) fields: Rc<[StateField]>,
    pub(crate) initial: Rc<[StateValue]>,
    pub(crate) reapply: Reapply,
    pub(crate) max_stacks: Option<NonZeroU32>,
}

/// An ability slot as the view read it: the rank of its ability, 0 while not learned, how many
/// ranks the ability has, and the filter of the units it may attack when it is a weapon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotRow {
    pub(crate) rank: u8,
    pub(crate) ranks: u8,
    pub(crate) weapon: Option<Filter>,
}

/// Fills the fields of a unit's row that a capability above the core holds.
pub(crate) type RowSource = fn(&EntityRef<'_>, &mut RowFill<'_>);

/// A row the view reads, as a capability fills it: its fields, the view's buffers of recent
/// attacks and ability slots, to which the row's runs are added, and the world the unit is in.
#[derive(Debug)]
pub(crate) struct RowFill<'a> {
    pub(crate) row: &'a mut UnitRow,
    pub(crate) world: &'a World,
    attacks: &'a mut Vec<RecentAttack>,
    slots: &'a mut Vec<SlotRow>,
    stats: &'a mut Vec<Num>,
    modifiers: &'a mut Vec<ModifierRow>,
    modifier_state: &'a mut Vec<StateValue>,
}

impl UnitRow {
    pub(crate) fn is_avatar(&self) -> bool {
        self.tags.tags.contains(EngineTag::Avatar.tag())
    }
}

impl RowFill<'_> {
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

    /// Adds `slots` to the row's run of ability slots.
    pub(crate) fn slotted(&mut self, slots: impl IntoIterator<Item = SlotRow>) {
        self.slots.extend(slots);
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
        self.units.clear();
        self.attacks.clear();
        self.slots.clear();
        self.stats.clear();
        self.modifiers.clear();
        self.modifier_state.clear();
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            let unit = world.entity(entity);
            let (Some(&pos), Some(&team)) = (unit.get::<Position>(), unit.get::<Team>()) else {
                continue;
            };
            let start = u32::try_from(self.attacks.len()).expect("attacks fit u32");
            let slots_start = u32::try_from(self.slots.len()).expect("slots fit u32");
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
                tracks: TrackSet::default(),
                tags: unit.get::<UnitTags>().copied().unwrap_or_default(),
                target: None,
                attack_range: None,
                attacks_start: start,
                attacks_end: start,
                slots_start,
                slots_end: slots_start,
                stats_start,
                stats_end: stats_start,
                modifiers_start,
                modifiers_end: modifiers_start,
            };
            let mut fill = RowFill {
                row: &mut row,
                world,
                attacks: &mut self.attacks,
                slots: &mut self.slots,
                stats: &mut self.stats,
                modifiers: &mut self.modifiers,
                modifier_state: &mut self.modifier_state,
            };
            for source in &self.sources {
                source(&unit, &mut fill);
            }
            row.attacks_end = u32::try_from(self.attacks.len()).expect("attacks fit u32");
            row.slots_end = u32::try_from(self.slots.len()).expect("slots fit u32");
            row.stats_end = u32::try_from(self.stats.len()).expect("stats fit u32");
            row.modifiers_end = u32::try_from(self.modifiers.len()).expect("modifiers fit u32");
            self.units.push(row);
        }
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
            damage_kinds: Rc::from([]),
            ability_names: Vec::new(),
            delivers: Vec::new(),
            spawns: Vec::new(),
            homing: Vec::new(),
            sources: Vec::new(),
            rate,
            now: Tick::ZERO,
            units: Vec::new(),
            relations: Relations::default(),
            metric: Metric::default(),
            attacks: Vec::new(),
            slots: Vec::new(),
            stat_names: Rc::from([]),
            pool_names: Rc::from([]),
            resource_names: Rc::from([]),
            track_names: Rc::from([]),
            stats: Vec::new(),
            modifier_info: Vec::new(),
            modifiers: Vec::new(),
            modifier_state: Vec::new(),
        })))
    }

    /// Reads the units of `world` for the phase that begins.
    pub(crate) fn read(&self, world: &World) {
        self.0.borrow_mut().read(world);
    }

    pub(crate) fn metric(&self) -> Metric {
        self.0.borrow().metric
    }

    pub(crate) fn types_mut(&self) -> RefMut<'_, UnitTypes> {
        RefMut::map(self.0.borrow_mut(), |view| &mut view.types)
    }

    /// Names the teams and the paths.
    pub(crate) fn set_names(&self, teams: Rc<Teams>, paths: Arc<[Box<str>]>) {
        let mut view = self.0.borrow_mut();
        view.teams = teams;
        view.paths = paths;
    }

    /// Sets the damage kinds the mode declares.
    pub(crate) fn set_damage_kinds(&self, damage_kinds: Rc<[DeclaredName]>) {
        self.0.borrow_mut().damage_kinds = damage_kinds;
    }

    /// `ms` in ticks at the match's rate, rounded up, at least one; an error for a negative time
    /// or one too long to count.
    pub(crate) fn ticks(&self, ms: INT) -> Checked<Ticks> {
        let ms = u64::try_from(ms)
            .ok()
            .ok_or_else(|| ApiError::NegativeTime.fail())?;
        let ticks = self.0.borrow().rate.ticks(ms);
        Ok(ticks
            .ok_or_else(|| ApiError::TimeTooLarge.fail())?
            .max(Ticks::ONE))
    }

    /// Adds the modifier the match loaded next, which takes the next id: modifiers load by
    /// package, then name.
    pub(crate) fn add_modifier(&self, info: ModifierInfo) {
        self.0.borrow_mut().modifier_info.push(info);
    }

    /// The modifier `name` of `package`; an error when it declares none.
    pub(crate) fn modifier(&self, package: u16, name: &str) -> Checked<ModifierId> {
        let view = self.0.borrow();
        let at = view
            .modifier_info
            .binary_search_by(|info| info.package.cmp(&package).then((*info.name).cmp(name)))
            .ok()
            .ok_or_else(|| ApiError::UnknownModifier.fail())?;
        Ok(ModifierId::new(
            u16::try_from(at).expect("modifiers fit u16"),
        ))
    }

    /// Whether the unit of `row` carries the modifier `name` of `package`.
    pub(crate) fn has_modifier(&self, row: &UnitRow, package: u16, name: &str) -> Checked<bool> {
        let id = self.modifier(package, name)?;
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
        let info = &view.modifier_info[id.index()];
        if let Some(handle) = handles.iter().find(|handle| handle.is(carrier, id, source)) {
            let mut data = handle.data();
            if data.removed {
                data.removed = false;
                data.written = false;
                data.stacks = 1;
                data.state.clone_from_slice(&info.initial);
            } else {
                data.stacks = info.reapply.stacks(data.stacks, info.max_stacks);
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
                let stacks = info.reapply.stacks(held.stacks, info.max_stacks);
                (stacks, state.to_vec())
            }
            None => (1, info.initial.to_vec()),
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
        let fields = Rc::clone(&self.0.borrow().modifier_info[id.index()].fields);
        ModifierHandle::new(carrier, id, source, stacks, state, fields, self.clone())
    }

    /// Sets the stats the mode declares, in the order units' runs of stats hold them.
    pub(crate) fn set_stat_names(&self, names: Rc<[Stat]>) {
        self.0.borrow_mut().stat_names = names;
    }

    /// How `of` regards `other`, as the units were read.
    pub(crate) fn attitude(&self, of: Team, other: Team) -> Attitude {
        self.0.borrow().relations.between(of, other)
    }

    /// The place of `stat` among the stats the mode declares; `None` when it does not declare it.
    pub(crate) fn stat_index(&self, stat: &Stat) -> Option<u16> {
        let at = self.0.borrow().stat_names.binary_search(stat).ok()?;
        Some(u16::try_from(at).expect("stats fit u16"))
    }

    /// The value of stat `name` of `row`; an error for a stat the mode does not declare, or a
    /// unit with no stats.
    pub(crate) fn stat(&self, row: &UnitRow, name: &str) -> Checked<Num> {
        let view = self.0.borrow();
        let at = Stat::named(name)
            .and_then(|stat| view.stat_names.binary_search(&stat).ok())
            .ok_or_else(|| ApiError::UnknownStat.fail())?;
        let run = &view.stats[row.stats_start as usize..row.stats_end as usize];
        run.get(at)
            .copied()
            .ok_or_else(|| ApiError::NoStats.fail().into())
    }

    /// Sets the pools the mode declares, by pool id.
    pub(crate) fn set_pool_names(&self, names: Rc<[DeclaredName]>) {
        self.0.borrow_mut().pool_names = names;
    }

    /// The pool `name`; an error for one the mode does not declare.
    pub(crate) fn pool(&self, name: &str) -> Checked<PoolId> {
        self.pool_id(name)
            .ok_or_else(|| ApiError::UnknownPool.fail().into())
    }

    /// The pool `name`; `None` for one the mode does not declare.
    pub(crate) fn pool_id(&self, name: &str) -> Option<PoolId> {
        let view = self.0.borrow();
        let at = view
            .pool_names
            .iter()
            .position(|pool| pool.as_str() == name)?;
        let pool = u8::try_from(at).ok().and_then(PoolId::new);
        Some(pool.expect("the load keeps the pools within the limit"))
    }

    /// Sets the players' resources the mode declares, by resource id.
    pub(crate) fn set_resource_names(&self, names: Rc<[DeclaredName]>) {
        self.0.borrow_mut().resource_names = names;
    }

    /// The player resource `name`; `None` for one the mode does not declare.
    pub(crate) fn resource(&self, name: &str) -> Option<ResourceId> {
        ResourceId::of(&self.0.borrow().resource_names, name)
    }

    /// How many player resources the mode declares.
    pub(crate) fn resource_count(&self) -> usize {
        self.0.borrow().resource_names.len()
    }

    /// What a cost named `name` takes from: a pool, or else a player resource; `None` for a
    /// name the mode declares neither as.
    pub(crate) fn cost_target(&self, name: &str) -> Option<CostTarget> {
        let pool = self.pool_id(name).map(CostTarget::Pool);
        pool.or_else(|| self.resource(name).map(CostTarget::Resource))
    }

    /// The damage kind `name`; an error for one the mode does not declare.
    pub(crate) fn damage_kind(&self, name: &str) -> Checked<DamageKind> {
        let view = self.0.borrow();
        let at = view
            .damage_kinds
            .iter()
            .position(|kind| kind.as_str() == name)
            .ok_or_else(|| ApiError::UnknownDamageKind.fail())?;
        Ok(DamageKind::new(
            u8::try_from(at).expect("the load keeps damage kinds within u8"),
        ))
    }

    /// Sets the tracks the mode declares.
    pub(crate) fn set_track_names(&self, track_names: Rc<[DeclaredName]>) {
        self.0.borrow_mut().track_names = track_names;
    }

    /// The track `name`; an error for one the mode does not declare.
    pub(crate) fn track(&self, name: &str) -> Checked<TrackId> {
        let view = self.0.borrow();
        let at = view
            .track_names
            .iter()
            .position(|track| track.as_str() == name)
            .ok_or_else(|| ApiError::UnknownTrack.fail())?;
        Ok(TrackId::new(at).expect("the load keeps tracks within their limit"))
    }

    /// The name of track `track`.
    pub(crate) fn track_name(&self, track: TrackId) -> ImmutableString {
        self.0.borrow().track_names[track.index()].as_str().into()
    }

    /// The name of damage kind `kind`.
    pub(crate) fn damage_kind_name(&self, kind: DamageKind) -> ImmutableString {
        self.0.borrow().damage_kinds[kind.index()].as_str().into()
    }

    /// Adds the name of the ability loaded next, which takes the next ability id, and how it
    /// delivers.
    pub(crate) fn add_ability(&self, name: &str, delivers: Option<Delivery>) {
        let mut view = self.0.borrow_mut();
        view.ability_names.push(name.into());
        view.delivers.push(delivers);
        view.spawns.push(None);
    }

    /// Binds ability `id` to the unit type it spawns.
    pub(crate) fn bind_spawn(&self, id: ActionId, unit_type: UnitType) {
        self.0.borrow_mut().spawns[id.index()] = Some(unit_type);
    }

    /// Marks `unit_type` as a projectile type that homes.
    pub(crate) fn set_homing(&self, unit_type: UnitType) {
        let homing = &mut self.0.borrow_mut().homing;
        let index = unit_type.index();
        if homing.len() <= index {
            homing.resize(index + 1, false);
        }
        homing[index] = true;
    }

    /// Whether the projectiles ability `id` launches home on a unit.
    pub(crate) fn launches_homing(&self, id: ActionId) -> bool {
        let view = self.0.borrow();
        let unit_type = view.spawns[id.index()].expect("a delivery binds its unit type");
        view.homing
            .get(unit_type.index())
            .is_some_and(|&homes| homes)
    }

    /// How ability `id` delivers, if other than at once.
    pub(crate) fn delivers(&self, id: ActionId) -> Option<Delivery> {
        self.0.borrow().delivers[id.index()]
    }

    /// The name of ability `id` in its package.
    pub(crate) fn ability_name(&self, id: ActionId) -> ImmutableString {
        self.0.borrow().ability_names[id.index()].clone()
    }

    /// Adds how a capability fills its fields of each row, after those added before it.
    pub(crate) fn add_source(&self, source: RowSource) {
        self.0.borrow_mut().sources.push(source);
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
    pub(crate) fn path(&self, name: &str) -> Option<PathId> {
        let view = self.0.borrow();
        let at = view.paths.iter().position(|held| **held == *name)?;
        Some(PathId::new(at))
    }

    /// The unit type named `name` in the mode's scope: one of the mode's, or an avatar.
    pub(crate) fn unit_type(&self, name: &str) -> Option<UnitType> {
        self.0.borrow().types.named(TypeScope::Mode, name)
    }

    /// The name of the unit type of `row`, `()` for a unit of no type.
    pub(crate) fn unit_type_name(&self, row: &UnitRow) -> Dynamic {
        let view = self.0.borrow();
        row.unit_type.map_or(Dynamic::UNIT, |unit_type| {
            Dynamic::from(ImmutableString::from(view.types.name(unit_type)))
        })
    }

    /// The run-time form of `filter`, its tag among those of the match's unit types.
    pub(crate) fn resolve_filter(&self, filter: &FilterData) -> Result<Filter, ApiError> {
        Filter::resolve(filter, &self.0.borrow().types)
    }

    /// The tag `name`; one the match does not have fails the call.
    pub(crate) fn tag(&self, name: &str) -> Result<Tag, ApiError> {
        self.0.borrow().types.tag(name).ok_or(ApiError::UnknownTag)
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
        let tag = self.tag(name).map_err(ApiError::fail)?;
        Ok(self.units_where(|row| row.tags.tags.contains(tag)))
    }

    /// Every avatar, living or dead, of `team` or of every team, by stable id.
    pub(crate) fn avatars(&self, team: Option<Team>) -> Array {
        self.units_where(|row| row.is_avatar() && team.is_none_or(|team| row.team == team))
    }

    pub(crate) fn row(&self, id: StableId) -> Option<UnitRow> {
        self.0.borrow().row(id)
    }

    /// Ability slot `slot` of the unit of `row`, when it has one.
    /// How many ability slots the unit of `row` has.
    pub(crate) const fn slot_count(row: &UnitRow) -> usize {
        (row.slots_end - row.slots_start) as usize
    }

    pub(crate) fn slot(&self, row: &UnitRow, slot: u8) -> Option<SlotRow> {
        let view = self.0.borrow();
        let run = &view.slots[row.slots_start as usize..row.slots_end as usize];
        run.get(usize::from(slot)).copied()
    }

    /// Whether a learned weapon of `row` selects `target`, as `row` regards it.
    pub(crate) fn armed_against(&self, row: &UnitRow, target: &UnitRow) -> bool {
        let view = self.0.borrow();
        let attitude = view.relations.between(row.team, target.team);
        let run = &view.slots[row.slots_start as usize..row.slots_end as usize];
        let target = Some((attitude, target.tags.tags));
        run.iter()
            .any(|slot| Action::arms(slot.rank, slot.weapon, target))
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
    pub(crate) fn param(&self, row: &UnitRow, name: &str) -> Option<Dynamic> {
        let view = self.0.borrow();
        let value = view.types.param(row.unit_type?, name)?;
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
        let window = view.rate.ticks(ms).unwrap_or(Ticks::new(u64::MAX));
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
    use super::*;

    impl View {
        /// How many unit types the match loaded, for a test to name the next one.
        pub(crate) fn types_count(&self) -> usize {
            self.0.borrow().types.count()
        }
    }
}
