use std::cell::{RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use bevy_ecs::world::{EntityRef, World};
use campfire_math::{Num, Vec3};
use campfire_script::rhai::{Array, Dynamic, Engine, INT, ImmutableString};
use campfire_sim::{EntityIndex, Position, SimTick, StableId, TickRate};

use crate::combat::attack_state::AttackState;
use crate::combat::attack_stats::{AttackStats, ground_offset};
use crate::combat::dead::Dead;
use crate::combat::health::Health;
use crate::combat::living_unit::LivingUnit;
use crate::combat::recent_attackers::{RecentAttack, RecentAttackers};
use crate::combat::team::Team;
use crate::units::error::{ApiError, Checked};
use crate::units::filter::Filter;
use crate::units::tag_set::{Tag, TagSet};
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;

/// What scripts see: the match's unit types, and its units, those with a team and health, as the
/// running phase of the tick began. The units are read again before each phase that runs
/// scripts, and every call of the phase sees them as they were read.
#[derive(Debug)]
pub(crate) struct ScriptView {
    types: UnitTypes,
    /// The name of each team, by index, once a mode sets them.
    teams: Vec<Box<str>>,
    /// The name of each lane, by index, once a mode sets them.
    lanes: Vec<Box<str>>,
    /// Reads the fields of the capabilities above the core.
    extras: fn(&EntityRef<'_>) -> RowExtras,
    rate: TickRate,
    /// The tick the units were read in.
    now: u64,
    /// By stable id.
    units: Vec<UnitRow>,
    /// The recent attacks on each unit, one run per unit.
    attacks: Vec<RecentAttack>,
}

/// A unit as the view read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnitRow {
    pub(crate) id: StableId,
    pub(crate) pos: Position,
    pub(crate) team: Team,
    pub(crate) alive: bool,
    pub(crate) unit_type: Option<UnitType>,
    pub(crate) target: Option<StableId>,
    pub(crate) attack_range: Option<Num>,
    pub(crate) extras: RowExtras,
    /// Its run of recent attacks, from `attacks_start` to `attacks_end`.
    attacks_start: u32,
    attacks_end: u32,
}

/// A unit's fields that capabilities above the core hold: the lane it walks or stands on, and the
/// player who controls it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RowExtras {
    pub(crate) lane: Option<u32>,
    pub(crate) owner: Option<u32>,
}

/// The view as the host and every handle share it.
#[derive(Clone)]
pub(crate) struct View(Rc<RefCell<ScriptView>>);

impl ScriptView {
    fn read(&mut self, world: &World) {
        self.now = world.resource::<SimTick>().get();
        self.units.clear();
        self.attacks.clear();
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            let unit = world.entity(entity);
            let (Some(&pos), Some(&team), true) = (
                unit.get::<Position>(),
                unit.get::<Team>(),
                unit.contains::<Health>(),
            ) else {
                continue;
            };
            let start = u32::try_from(self.attacks.len()).expect("attacks fit u32");
            if let Some(recent) = unit.get::<RecentAttackers>() {
                self.attacks.extend(recent.iter());
            }
            let end = u32::try_from(self.attacks.len()).expect("attacks fit u32");
            self.units.push(UnitRow {
                id,
                pos,
                team,
                alive: !unit.contains::<Dead>(),
                unit_type: unit.get::<UnitType>().copied(),
                target: unit.get::<AttackState>().and_then(|attack| attack.target()),
                attack_range: unit.get::<AttackStats>().map(|stats| stats.range()),
                extras: (self.extras)(&unit),
                attacks_start: start,
                attacks_end: end,
            });
        }
    }

    fn row(&self, id: StableId) -> Option<UnitRow> {
        let index = self.units.binary_search_by_key(&id, |row| row.id).ok()?;
        Some(self.units[index])
    }

    fn tags(&self, row: &UnitRow) -> TagSet {
        row.unit_type
            .map_or(TagSet::default(), |unit_type| self.types.tags(unit_type))
    }

    /// The living units `filter` selects relative to `of`.
    fn selected<'a>(
        &'a self,
        of: &UnitRow,
        filter: &str,
    ) -> Result<impl Iterator<Item = &'a UnitRow>, ApiError> {
        let filter = Filter::parse(filter, &self.types)?;
        let of = of.team;
        Ok(self
            .units
            .iter()
            .filter(move |row| row.alive && filter.selects(of, row.team, self.tags(row))))
    }
}

impl View {
    pub(crate) fn new(rate: TickRate) -> View {
        View(Rc::new(RefCell::new(ScriptView {
            types: UnitTypes::default(),
            teams: Vec::new(),
            lanes: Vec::new(),
            extras: |_| RowExtras::default(),
            rate,
            now: 0,
            units: Vec::new(),
            attacks: Vec::new(),
        })))
    }

    /// Reads the units of `world` for the phase that begins.
    pub(crate) fn read(&self, world: &World) {
        self.0.borrow_mut().read(world);
    }

    pub(crate) fn types_mut(&self) -> RefMut<'_, UnitTypes> {
        RefMut::map(self.0.borrow_mut(), |view| &mut view.types)
    }

    /// Names the teams and the lanes, and sets how rows read the fields above the core.
    pub(crate) fn set_names(
        &self,
        teams: Vec<Box<str>>,
        lanes: Vec<Box<str>>,
        extras: fn(&EntityRef<'_>) -> RowExtras,
    ) {
        let mut view = self.0.borrow_mut();
        view.teams = teams;
        view.lanes = lanes;
        view.extras = extras;
    }

    /// The name of `team`.
    pub(crate) fn team_name(&self, team: Team) -> Checked<Dynamic> {
        let view = self.0.borrow();
        let name = view.teams.get(usize::from(team.index()));
        name.map(|name| Dynamic::from(ImmutableString::from(&**name)))
            .ok_or_else(|| ApiError::UnknownTeam.fail().into())
    }

    /// The name of `lane`, `()` for none.
    pub(crate) fn lane_name(&self, lane: Option<u32>) -> Dynamic {
        let view = self.0.borrow();
        lane.and_then(|lane| view.lanes.get(lane as usize))
            .map_or(Dynamic::UNIT, |name| {
                Dynamic::from(ImmutableString::from(&**name))
            })
    }

    /// The tag `name`; one no unit type declares fails the call.
    pub(crate) fn tag(&self, name: &str) -> Result<Tag, ApiError> {
        self.0.borrow().types.tag(name).ok_or(ApiError::UnknownTag)
    }

    pub(crate) fn has_tag(&self, row: &UnitRow, tag: Tag) -> bool {
        self.0.borrow().tags(row).contains(tag)
    }

    /// Every unit, living or dead, that `keep` keeps, by stable id.
    pub(crate) fn units_where(&self, mut keep: impl FnMut(&ScriptView, &UnitRow) -> bool) -> Array {
        let view = self.0.borrow();
        view.units
            .iter()
            .filter(|row| keep(&view, row))
            .map(|row| Dynamic::from(Unit::new(row.id, self.clone())))
            .collect()
    }

    /// Every unit, living or dead, with the tag `name`, by stable id.
    pub(crate) fn units_tagged(&self, name: &str) -> Checked<Array> {
        let tag = self.tag(name).map_err(ApiError::fail)?;
        Ok(self.units_where(|view, row| view.tags(row).contains(tag)))
    }

    /// Every hero, living or dead, of `team` or of every team, by stable id.
    pub(crate) fn heroes(&self, team: Option<Team>) -> Array {
        self.units_where(|view, row| {
            let hero = view.types.hero();
            hero.is_some_and(|hero| view.tags(row).contains(hero))
                && team.is_none_or(|team| row.team == team)
        })
    }

    pub(crate) fn row(&self, id: StableId) -> Option<UnitRow> {
        self.0.borrow().row(id)
    }

    /// The handle of unit `id`, when the view read it.
    pub(crate) fn unit(&self, id: StableId) -> Option<Unit> {
        self.row(id).map(|_| Unit::new(id, self.clone()))
    }

    /// Unit `id`, when it is a living unit.
    pub(crate) fn living(&self, id: StableId) -> Option<LivingUnit> {
        let row = self.row(id).filter(|row| row.alive)?;
        Some(LivingUnit {
            id,
            pos: row.pos,
            team: row.team,
        })
    }

    pub(crate) fn is_hero(&self, row: &UnitRow) -> bool {
        let view = self.0.borrow();
        view.types
            .hero()
            .is_some_and(|hero| view.tags(row).contains(hero))
    }

    /// The param `name` of the unit type of `row`.
    pub(crate) fn param(&self, row: &UnitRow, name: &str) -> Option<Dynamic> {
        let view = self.0.borrow();
        let value = view.types.param(row.unit_type?, name)?;
        Some(value.to_dynamic())
    }

    /// The living units within `radius` of `pos` on the ground plane that `filter` selects
    /// relative to `of`, by stable id.
    pub(crate) fn find(
        &self,
        of: &Unit,
        pos: Position,
        radius: Num,
        filter: &str,
    ) -> Checked<Array> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        let view = self.0.borrow();
        let of = of.row();
        let selected = view.selected(&of, filter).map_err(ApiError::fail)?;
        Ok(selected
            .filter(|row| Vec3::ZERO.within(ground_offset(pos, row.pos), radius))
            .map(|row| Dynamic::from(Unit::new(row.id, self.clone())))
            .collect())
    }

    /// The nearest living unit within `radius` of `of` on the ground plane that `filter` selects
    /// relative to it, by exact distance, the lower stable id on a tie; `()` when there is none.
    /// Every unit is visible until the match has vision.
    pub(crate) fn nearest_visible(&self, of: &Unit, radius: Num, filter: &str) -> Checked<Dynamic> {
        if radius < Num::ZERO {
            return Err(ApiError::NegativeRadius.fail().into());
        }
        let view = self.0.borrow();
        let of = of.row();
        let nearest = view
            .selected(&of, filter)
            .map_err(ApiError::fail)?
            .map(|row| (ground_offset(of.pos, row.pos), row.id))
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
        let window = view.rate.ticks(ms).unwrap_or(u64::MAX);
        let row = unit.row();
        let run = &view.attacks[row.attacks_start as usize..row.attacks_end as usize];
        Ok(run
            .iter()
            .filter(|attack| {
                // A strike later than the view's tick, as a rollback can leave, is not recent.
                view.now
                    .checked_sub(attack.tick)
                    .is_some_and(|age| age <= window)
            })
            .filter(|attack| view.row(attack.source).is_some_and(|source| source.alive))
            .map(|attack| Dynamic::from(Unit::new(attack.source, self.clone())))
            .collect())
    }

    /// Registers the queries on the `ctx` of type `C`: `find` and `nearest_visible`.
    pub(crate) fn register_queries<C: Clone + 'static>(engine: &mut Engine, view: fn(&C) -> &View) {
        engine
            .register_fn(
                "find",
                move |ctx: &mut C, of: Unit, pos: Position, radius: Num, filter: &str| {
                    view(ctx).find(&of, pos, radius, filter)
                },
            )
            .register_fn(
                "find",
                move |ctx: &mut C, of: Unit, pos: Position, radius: INT, filter: &str| {
                    view(ctx).find(&of, pos, ApiError::num(radius)?, filter)
                },
            )
            .register_fn(
                "nearest_visible",
                move |ctx: &mut C, of: Unit, radius: Num, filter: &str| {
                    view(ctx).nearest_visible(&of, radius, filter)
                },
            )
            .register_fn(
                "nearest_visible",
                move |ctx: &mut C, of: Unit, radius: INT, filter: &str| {
                    view(ctx).nearest_visible(&of, ApiError::num(radius)?, filter)
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
