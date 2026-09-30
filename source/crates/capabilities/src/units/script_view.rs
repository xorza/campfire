use std::cell::{RefCell, RefMut};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use bevy_ecs::world::{EntityRef, World};
use campfire_math::{Num, Vec3};
use campfire_script::rhai::{Array, Dynamic, Engine, INT, ImmutableString};
use campfire_sim::{EntityIndex, PlayerSlot, Position, SimTick, StableId, Tick, TickRate, Ticks};

use crate::scripts::error::{ApiError, Checked};
use crate::units::filter::Filter;
use crate::units::lane::Lane;
use crate::units::living_unit::LivingUnit;
use crate::units::owner::Owner;
use crate::units::recent_attack::RecentAttack;
use crate::units::tag_set::{Tag, TagSet};
use crate::units::team::Team;
use crate::units::team_set::TeamSet;
use crate::units::teams::Teams;
use crate::units::unit::Unit;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::values::filter_data::FilterData;

/// What scripts see: the match's unit types, and its units, those with a team, as the running
/// phase of the tick began. The units are read again before each phase that runs
/// scripts, and every call of the phase sees them as they were read.
#[derive(Debug)]
pub(crate) struct ScriptView {
    types: UnitTypes,
    /// The match's teams, once a mode sets them.
    teams: Rc<Teams>,
    /// The name of each lane, by index, once a mode sets them.
    lanes: Arc<[Box<str>]>,
    /// How each installed capability above the core fills its fields of a row, in install order.
    sources: Vec<RowSource>,
    rate: TickRate,
    /// The tick the units were read in.
    now: Tick,
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
    pub(crate) unit_type: Option<UnitType>,
    /// The player who controls it.
    pub(crate) owner: Option<PlayerSlot>,
    /// Whether it is not dead; `combat` fills it, and the next two.
    pub(crate) alive: bool,
    pub(crate) target: Option<StableId>,
    pub(crate) attack_range: Option<Num>,
    /// The lane it walks or stands on; `navigation` fills it.
    pub(crate) lane: Option<Lane>,
    /// The teams that see it; `vision` fills it, and without vision every team does.
    pub(crate) seen_by: TeamSet,
    /// Its run of recent attacks, from `attacks_start` to `attacks_end`.
    attacks_start: u32,
    attacks_end: u32,
}

/// Fills the fields of a unit's row that a capability above the core holds.
pub(crate) type RowSource = fn(&EntityRef<'_>, &mut RowFill<'_>);

/// A row the view reads, as a capability fills it: its fields, and the view's buffer of recent
/// attacks, to which the row's run is added.
#[derive(Debug)]
pub(crate) struct RowFill<'a> {
    pub(crate) row: &'a mut UnitRow,
    attacks: &'a mut Vec<RecentAttack>,
}

impl RowFill<'_> {
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
        self.units.clear();
        self.attacks.clear();
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            let unit = world.entity(entity);
            let (Some(&pos), Some(&team)) = (unit.get::<Position>(), unit.get::<Team>()) else {
                continue;
            };
            let start = u32::try_from(self.attacks.len()).expect("attacks fit u32");
            let mut row = UnitRow {
                id,
                pos,
                team,
                alive: true,
                unit_type: unit.get::<UnitType>().copied(),
                owner: unit.get::<Owner>().map(|owner| owner.slot()),
                lane: None,
                seen_by: TeamSet::ALL,
                target: None,
                attack_range: None,
                attacks_start: start,
                attacks_end: start,
            };
            let mut fill = RowFill {
                row: &mut row,
                attacks: &mut self.attacks,
            };
            for source in &self.sources {
                source(&unit, &mut fill);
            }
            row.attacks_end = u32::try_from(self.attacks.len()).expect("attacks fit u32");
            self.units.push(row);
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
            teams: Rc::default(),
            lanes: Arc::default(),
            sources: Vec::new(),
            rate,
            now: Tick::ZERO,
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

    /// Names the teams and the lanes.
    pub(crate) fn set_names(&self, teams: Rc<Teams>, lanes: Arc<[Box<str>]>) {
        let mut view = self.0.borrow_mut();
        view.teams = teams;
        view.lanes = lanes;
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

    /// The name of `lane`, `()` for none.
    pub(crate) fn lane_name(&self, lane: Option<Lane>) -> Dynamic {
        let view = self.0.borrow();
        lane.and_then(|lane| view.lanes.get(lane.index()))
            .map_or(Dynamic::UNIT, |name| {
                Dynamic::from(ImmutableString::from(&**name))
            })
    }

    /// The lane named `name`.
    pub(crate) fn lane(&self, name: &str) -> Option<Lane> {
        let view = self.0.borrow();
        let at = view.lanes.iter().position(|held| **held == *name)?;
        Some(Lane::new(at))
    }

    /// The unit type named `name`.
    pub(crate) fn unit_type(&self, name: &str) -> Option<UnitType> {
        self.0.borrow().types.named(name)
    }

    /// The name of the unit type of `row`, `()` for a unit of no type.
    pub(crate) fn unit_type_name(&self, row: &UnitRow) -> Dynamic {
        let view = self.0.borrow();
        row.unit_type.map_or(Dynamic::UNIT, |unit_type| {
            Dynamic::from(ImmutableString::from(view.types.name(unit_type)))
        })
    }

    /// The tags of units of `unit_type`; none for a unit of no type.
    pub(crate) fn type_tags(&self, unit_type: Option<UnitType>) -> TagSet {
        let view = self.0.borrow();
        unit_type.map_or(TagSet::default(), |unit_type| view.types.tags(unit_type))
    }

    /// The run-time form of `filter`, its tag among those of the match's unit types.
    pub(crate) fn resolve_filter(&self, filter: &FilterData) -> Result<Filter, ApiError> {
        Filter::resolve(filter, &self.0.borrow().types)
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
            .filter(|row| pos.within_ground(row.pos, radius))
            .map(|row| Dynamic::from(Unit::new(row.id, self.clone())))
            .collect())
    }

    /// The nearest living unit within `radius` of `of` on the ground plane that `filter` selects
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
            .map(|row| (of.pos.ground_offset(row.pos), row.id))
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

    /// Registers the queries on the `ctx` of type `C`: `find`, `find_visible` and
    /// `nearest_visible`.
    pub(crate) fn register_queries<C: Clone + 'static>(engine: &mut Engine, view: fn(&C) -> &View) {
        for (name, visible) in [("find", false), ("find_visible", true)] {
            engine
                .register_fn(
                    name,
                    move |ctx: &mut C, of: Unit, pos: Position, radius: Num, filter: &str| {
                        view(ctx).find(&of, pos, radius, filter, visible)
                    },
                )
                .register_fn(
                    name,
                    move |ctx: &mut C, of: Unit, pos: Position, radius: INT, filter: &str| {
                        view(ctx).find(&of, pos, ApiError::num(radius)?, filter, visible)
                    },
                );
        }
        engine
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
