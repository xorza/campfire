use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_geometry::{Bounds, Metric, Shape};
use campfire_math::Num;
use campfire_sim::{Position, StableId, TickRate};
use campfire_values::Relation;

use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::scripts::script_list::ScriptList;
use crate::scripts::script_value::ScriptValue;
use crate::units::action_id::ActionId;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::living_unit::LivingUnit;
use crate::units::row_fill::RowFill;
use crate::units::row_reads::RowReads;
use crate::units::tag::Tag;
use crate::units::target_index::{TargetIndex, TargetQuery};
use crate::units::team::Team;
use crate::units::unit::Unit;
use crate::units::unit_row::UnitRow;
use crate::units::unit_rows::UnitRows;
use crate::units::unit_types::UnitTypes;
use crate::units::view_column::{ViewColumn, ViewColumns};
use crate::units::view_names::ViewNames;

/// What scripts see, as the host and every handle share it: the match's names, its units, those
/// with a team, as the running phase of the tick began, and the targets' bodies the queries that
/// reach by distance search. The units are read again before each phase that runs scripts, and
/// every call of the phase sees them as they were read. Each part has a cell of its own, so a
/// name lookup does not borrow the rows.
#[derive(Clone)]
pub struct View(Rc<ViewParts>);

/// The parts of the view.
#[derive(Debug)]
struct ViewParts {
    rate: TickRate,
    names: RefCell<ViewNames>,
    rows: RefCell<UnitRows>,
    targets: RefCell<TargetIndex>,
}

impl View {
    /// The view of a match at `rate`, its names the engine's tags `engine` alone, until the load
    /// gives the match's.
    pub fn new(rate: TickRate, engine: impl IntoIterator<Item = EngineTag>) -> View {
        View(Rc::new(ViewParts {
            rate,
            names: RefCell::new(ViewNames::new(UnitTypes::new(engine))),
            rows: RefCell::default(),
            targets: RefCell::default(),
        }))
    }

    /// The names scripts read, as the load and the mode set them.
    pub fn names(&self) -> Ref<'_, ViewNames> {
        self.0.names.borrow()
    }

    /// The names, for the load and the mode to set.
    pub fn names_mut(&self) -> RefMut<'_, ViewNames> {
        self.0.names.borrow_mut()
    }

    /// The units as the running phase read them.
    pub fn rows(&self) -> Ref<'_, UnitRows> {
        self.0.rows.borrow()
    }

    /// Reads the units of `world` for the phase that begins.
    pub fn read(&self, world: &mut World) {
        self.0.rows.borrow_mut().read(world);
        self.0.targets.borrow_mut().forget();
    }

    /// The map's bounds, as the units were read.
    pub fn bounds(&self) -> Bounds {
        self.rows().bounds()
    }

    pub fn metric(&self) -> Metric {
        self.rows().metric()
    }

    /// The tick the units were read in.
    pub fn now(&self) -> Tick {
        self.rows().now()
    }

    /// The match's tick rate.
    pub fn rate(&self) -> TickRate {
        self.0.rate
    }

    /// `ms` in ticks at the match's rate, rounded up, at least one, or all ticks for a time too
    /// long to count: a window back from now.
    pub fn window(&self, ms: u64) -> Ticks {
        self.0.rate.window(ms)
    }

    /// `ms` in ticks at the match's rate, rounded up, at least one; an error for a negative time
    /// or one too long to count.
    pub fn ticks(&self, ms: i64) -> Result<Ticks, ApiError> {
        let ms = u64::try_from(ms).ok().ok_or(ApiError::NegativeTime)?;
        self.0.rate.duration(ms).ok_or(ApiError::TimeTooLarge)
    }

    /// `ms`, more than 0, in ticks as `ticks` gives them: what lasts, as a reveal or a knock
    /// back, lasts some time.
    pub fn lasting(&self, ms: i64) -> Result<Ticks, ApiError> {
        if ms == 0 {
            return Err(ApiError::ZeroTime);
        }
        self.ticks(ms)
    }

    /// How `of` regards `other`, as the units were read.
    pub fn relation(&self, of: Team, other: Team) -> Relation {
        self.rows().relations().between(of, other)
    }

    /// Adds how a capability fills its column of each row, and its fields of the core row,
    /// from the parts `D` of `world`'s units, after those added before it. Its column comes
    /// first.
    pub fn add_source<D: RowReads, C: ViewColumn>(
        &self,
        world: &mut World,
        fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>),
    ) {
        self.0.rows.borrow_mut().add_source::<D, C>(world, fill);
    }

    /// Adds `column`, which a source fills.
    pub fn add_column<C: ViewColumn>(&self, column: C) {
        self.0.rows.borrow_mut().add_column(column);
    }

    /// What `read` gives of the column of type `C`; `None` when none was added.
    pub fn column<C: ViewColumn, R>(&self, read: impl FnOnce(&C) -> R) -> Option<R> {
        self.rows().columns().get().map(read)
    }

    /// Changes the column of type `C` by `write`, when one was added.
    /// The next read fills every row again, as the rows may derive from what `write` changes.
    pub fn column_mut<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        if let Some(column) = self.0.rows.borrow_mut().column_mut(true) {
            write(column);
        }
    }

    /// Makes the next read fill every row. Bevy clamps each change tick older than it compares
    /// exactly as it checks the world's ticks, but not the last read of the view's queries of
    /// changed parts, so after a check those queries may miss a change.
    pub(crate) fn refill_next(&self) {
        self.0.rows.borrow_mut().refill_next();
    }

    /// Changes the rows of the column of type `C` by `write`, as a call's effect changes the
    /// units' parts too, which the next read finds changed.
    pub(crate) fn write_rows<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        if let Some(column) = self.0.rows.borrow_mut().column_mut(false) {
            write(column);
        }
    }

    /// The place among the rows of unit `id`, when the view read it.
    pub fn row_index(&self, id: StableId) -> Option<usize> {
        self.rows().index(id)
    }

    /// Every unit, living or dead, that `keep` keeps, by stable id.
    fn units_where(&self, mut keep: impl FnMut(&UnitRow) -> bool) -> ScriptList {
        let rows = self.rows();
        rows.units()
            .iter()
            .enumerate()
            .filter(|(_, row)| keep(row))
            .map(|(at, row)| ScriptValue::of(Unit::new(row.id, at, self.clone())))
            .collect()
    }

    /// Every unit, living or dead, with `tag`, by stable id.
    pub fn units_tagged(&self, tag: Tag) -> ScriptList {
        self.units_where(|row| row.tags.tags.contains(tag))
    }

    /// Every avatar, living or dead, of `team` or of every team, by stable id.
    pub fn avatars(&self, team: Option<Team>) -> ScriptList {
        self.units_where(|row| row.is_avatar() && team.is_none_or(|team| row.team == team))
    }

    pub fn row(&self, id: StableId) -> Option<UnitRow> {
        self.rows().row(id).copied()
    }

    /// What `read` reads of the row at `at` among the rows, as a handle names it, in place.
    pub fn read_row<R>(&self, at: usize, read: impl FnOnce(&UnitRow) -> R) -> R {
        read(&self.rows().units()[at])
    }

    /// Calls `visit` with each row the view read, and its place, in order of stable id.
    pub fn each_row(&self, mut visit: impl FnMut(usize, &UnitRow)) {
        for (at, row) in self.rows().units().iter().enumerate() {
            visit(at, row);
        }
    }

    /// The handle of unit `id`, when the view read it.
    pub fn unit(&self, id: StableId) -> Option<Unit> {
        let at = self.row_index(id)?;
        Some(Unit::new(id, at, self.clone()))
    }

    /// The handle of unit `id` as a script value: `()` for none, or a unit the view did not read.
    pub fn unit_value(&self, id: Option<StableId>) -> ScriptValue {
        id.and_then(|id| self.unit(id))
            .map_or(ScriptValue::UNIT, ScriptValue::of)
    }

    /// The name of action `id` as a script value: `()` for none.
    pub fn ability_value(&self, id: Option<ActionId>) -> ScriptValue {
        id.map_or(ScriptValue::UNIT, |id| {
            let name = self.names().actions.name(id);
            ScriptValue::of(name.expect("an action of the match is named"))
        })
    }

    /// Unit `id`, when it is a living unit that may be a target.
    pub fn living(&self, id: StableId) -> Option<LivingUnit> {
        let row = self.row(id).filter(|row| row.targetable)?;
        Some(LivingUnit {
            id,
            pos: row.pos,
            team: row.team,
            shape: row.shape,
            tags: row.tags.tags,
        })
    }

    /// The living targets whose bodies come within `radius` of `pos` in the map's metric, as an
    /// area of that radius reaches, that `filter` selects relative to `of`, and `seen` lets by
    /// the view's columns and their rows, by stable id.
    pub fn find(
        &self,
        of: &Unit,
        pos: Position,
        radius: Num,
        filter: &Filter,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> Checked<ScriptList> {
        let radius = ApiError::radius(radius)?;
        let rows = self.rows();
        let units = rows.units();
        let query = TargetQuery {
            team: units[of.row_index()].team,
            from: pos,
            shape: Shape::POINT,
            radius,
            filter,
        };
        let mut targets = self.0.targets.borrow_mut();
        let found = targets.find(&rows, query, seen);
        let unit =
            |&index: &usize| ScriptValue::of(Unit::new(units[index].id, index, self.clone()));
        Ok(found.iter().map(unit).collect())
    }

    /// The nearest living target that `radius` from the edge of `of`'s body reaches in the map's
    /// metric, as a weapon's range does, that `filter` selects relative to it and `seen` lets by
    /// the view's columns and its row, by exact distance between centres, the lower stable id on
    /// a tie; `()` when there is none.
    pub fn nearest(
        &self,
        of: &Unit,
        radius: Num,
        filter: &Filter,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> Checked<ScriptValue> {
        let radius = ApiError::radius(radius)?;
        let rows = self.rows();
        let of = rows.units()[of.row_index()];
        let query = TargetQuery {
            team: of.team,
            from: of.pos,
            shape: of.shape,
            radius,
            filter,
        };
        let metric = rows.metric();
        let mut nearest = None;
        self.0
            .targets
            .borrow_mut()
            .visit(&rows, query, seen, |body| {
                let distance = metric.offset(of.pos, body.pos).length_squared_bits();
                let candidate = (distance, body.id, body.key);
                if nearest.is_none_or(|best| candidate < best) {
                    nearest = Some(candidate);
                }
            });
        Ok(nearest.map_or(ScriptValue::UNIT, |(_, id, at)| {
            ScriptValue::of(Unit::new(id, at, self.clone()))
        }))
    }

    /// `ctx.find`.
    pub fn register_queries(api: &mut ApiBuilder<'_>) {
        let find = MemberSpec::call(
            "find",
            &[&["of", "pos", "radius", "filter"]],
            "the living targets whose bodies come within `radius` of `pos`, as an area's, that `filter` selects for `of`, seen or not, by stable id",
        );
        api.call(
            find,
            |ctx: &mut Ctx, of: Unit, pos: Position, radius: Num, filter: Filter| {
                ctx.view().find(&of, pos, radius, &filter, |_, _| true)
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

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::cell::RefMut;

    use campfire_values::FilterData;

    use crate::scripts::error::UnknownName;
    use crate::units::filter::Filter;
    use crate::units::unit_types::UnitTypes;
    use crate::units::view::View;
    use crate::units::view_names::ViewNames;

    impl View {
        /// The run-time form of `filter`, its tag among those of the match's unit types.
        pub fn resolve_filter(&self, filter: &FilterData) -> Result<Filter, UnknownName> {
            Filter::resolve(filter, self.names().types())
        }

        /// How many unit types the match loaded, for a test to name the next one.
        pub fn types_count(&self) -> usize {
            self.names().types().count()
        }

        pub fn types_mut(&self) -> RefMut<'_, UnitTypes> {
            RefMut::map(self.0.names.borrow_mut(), ViewNames::types_mut)
        }
    }
}
