use std::cell::{Ref, RefCell};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use bevy_ecs::query::ROQueryItem;
use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_math::Num;
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_sim::{Position, StableId, TickRate};

use crate::geometry::bounds::Bounds;
use crate::geometry::metric::Metric;
use crate::geometry::shape::Shape;
use crate::players::resource_id::ResourceId;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_api::member_spec::MemberSpec;
use crate::units::action_id::ActionId;
use crate::units::filter::Filter;
use crate::units::living_unit::LivingUnit;
use crate::units::path_id::PathId;
use crate::units::row_fill::RowFill;
use crate::units::row_parts::RowParts;
use crate::units::tag::Tag;
use crate::units::target_index::{TargetIndex, TargetQuery};
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::track_id::TrackId;
use crate::units::unit::Unit;
use crate::units::unit_row::UnitRow;
use crate::units::unit_rows::UnitRows;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::units::view_column::{ViewColumn, ViewColumns};
use crate::units::view_names::ViewNames;
use crate::values::attitude::Attitude;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::name_list::NameList;

/// What scripts see, as the host and every handle share it: the match's names, its units, those
/// with a team, as the running phase of the tick began, and the targets' bodies the queries that
/// reach by distance search. The units are read again before each phase that runs scripts, and
/// every call of the phase sees them as they were read. Each part has a cell of its own, so a
/// name lookup does not borrow the rows.
#[derive(Clone)]
pub(crate) struct View(Rc<ViewParts>);

/// The parts of the view.
#[derive(Debug)]
struct ViewParts {
    rate: TickRate,
    names: RefCell<ViewNames>,
    rows: RefCell<UnitRows>,
    targets: RefCell<TargetIndex>,
}

impl View {
    pub(crate) fn new(rate: TickRate) -> View {
        View(Rc::new(ViewParts {
            rate,
            names: RefCell::default(),
            rows: RefCell::default(),
            targets: RefCell::default(),
        }))
    }

    /// The names scripts read, as the load and the mode set them.
    pub(crate) fn names(&self) -> Ref<'_, ViewNames> {
        self.0.names.borrow()
    }

    /// The units as the running phase read them.
    pub(crate) fn rows(&self) -> Ref<'_, UnitRows> {
        self.0.rows.borrow()
    }

    /// Reads the units of `world` for the phase that begins.
    pub(crate) fn read(&self, world: &mut World) {
        self.0.rows.borrow_mut().read(world);
        self.0.targets.borrow_mut().forget();
    }

    /// The map's bounds, as the units were read.
    pub(crate) fn bounds(&self) -> Bounds {
        self.rows().bounds()
    }

    pub(crate) fn metric(&self) -> Metric {
        self.rows().metric()
    }

    /// Whether the match loaded `unit_type`.
    pub(crate) fn has_type(&self, unit_type: UnitType) -> bool {
        self.names().has_type(unit_type)
    }

    /// Whether `kind` is one of the mode's damage kinds; any is, before a mode names them.
    pub(crate) fn has_damage_kind(&self, kind: DamageKind) -> bool {
        self.names().has_damage_kind(kind)
    }

    /// See `ViewNames::has_team`.
    pub(crate) fn has_team(&self, team: Team) -> bool {
        self.names().has_team(team)
    }

    /// See `ViewNames::has_player`.
    pub(crate) fn has_player(&self, slot: PlayerSlot) -> bool {
        self.names().has_player(slot)
    }

    /// See `ViewNames::fits_resources`.
    pub(crate) fn fits_resources(&self, resources: usize, amounts: usize) -> bool {
        self.names().fits_resources(resources, amounts)
    }

    /// Player `player`'s slot, as a script names it, when the session has it.
    pub(crate) fn player(&self, player: INT) -> Checked<PlayerSlot> {
        self.names().player(player)
    }

    /// Names the teams and the paths.
    pub(crate) fn set_names(&self, teams: Rc<Teams>, paths: Arc<NameList>) {
        self.0.names.borrow_mut().set_teams(teams, paths);
    }

    /// Sets the damage kinds and the players' resources the mode declares, by id.
    pub(crate) fn set_mode_names(
        &self,
        damage_kinds: &[DeclaredName],
        resources: Arc<[DeclaredName]>,
    ) {
        self.0
            .names
            .borrow_mut()
            .set_mode_names(damage_kinds, resources);
    }

    /// The tick the units were read in.
    pub(crate) fn now(&self) -> Tick {
        self.rows().now()
    }

    /// The match's tick rate.
    pub(crate) fn rate(&self) -> TickRate {
        self.0.rate
    }

    /// `ms` in ticks at the match's rate, rounded up, at least one, or all ticks for a time too
    /// long to count: a window back from now.
    pub(crate) fn window(&self, ms: u64) -> Ticks {
        self.0.rate.window(ms)
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
        self.0.rate.duration(ms).ok_or(ApiError::TimeTooLarge)
    }

    /// Sets the match's unit types and tags, as the load built them.
    pub(crate) fn set_types(&self, types: UnitTypes) {
        self.0.names.borrow_mut().set_types(types);
    }

    /// How `of` regards `other`, as the units were read.
    pub(crate) fn attitude(&self, of: Team, other: Team) -> Attitude {
        self.rows().relations().between(of, other)
    }

    /// The player resource `name`; `None` for one the mode does not declare.
    pub(crate) fn resource_named(&self, name: &str) -> Option<ResourceId> {
        self.names().resource_named(name)
    }

    /// The damage kind `name`; an error for one the mode does not declare.
    pub(crate) fn damage_kind_named(&self, name: &str) -> Checked<DamageKind> {
        self.names().damage_kind_named(name)
    }

    /// Names the tracks the mode declares, by track id.
    pub(crate) fn set_track_names<'a>(&self, names: impl Iterator<Item = &'a str>) {
        self.0.names.borrow_mut().set_track_names(names);
    }

    /// The name of track `track`.
    pub(crate) fn track_name(&self, track: TrackId) -> ImmutableString {
        self.names().track_name(track)
    }

    /// The name of damage kind `kind`.
    pub(crate) fn damage_kind_name(&self, kind: DamageKind) -> ImmutableString {
        self.names().damage_kind_name(kind)
    }

    /// Names the match's actions, by action id.
    pub(crate) fn set_action_names<'a>(&self, names: impl Iterator<Item = &'a str>) {
        self.0.names.borrow_mut().set_action_names(names);
    }

    /// The name of ability `id` in its package.
    pub(crate) fn ability_name(&self, id: ActionId) -> ImmutableString {
        self.names().ability_name(id)
    }

    /// Adds how a capability fills its column of each row, and its fields of the core row,
    /// from the parts `D` of `world`'s units, after those added before it. Its column comes
    /// first.
    pub(crate) fn add_source<D: RowParts, C: ViewColumn>(
        &self,
        world: &mut World,
        fill: for<'w, 's> fn(ROQueryItem<'w, 's, D>, &mut RowFill<'_, C>),
    ) {
        self.0.rows.borrow_mut().add_source::<D, C>(world, fill);
    }

    /// Adds `column`, which a source fills.
    pub(crate) fn add_column<C: ViewColumn>(&self, column: C) {
        self.0.rows.borrow_mut().add_column(column);
    }

    /// What `read` gives of the column of type `C`; `None` when none was added.
    pub(crate) fn column<C: ViewColumn, R>(&self, read: impl FnOnce(&C) -> R) -> Option<R> {
        self.rows().columns().get().map(read)
    }

    /// Changes the column of type `C` by `write`, when one was added.
    /// The next read fills every row again, as the rows may derive from what `write` changes.
    pub(crate) fn column_mut<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
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
    pub(crate) fn row_index(&self, id: StableId) -> Option<usize> {
        self.rows().index(id)
    }

    /// The name of `team`.
    pub(crate) fn team_name(&self, team: Team) -> Checked<Dynamic> {
        self.names().team_name(team)
    }

    /// The name of `path`, `()` for none.
    pub(crate) fn path_name(&self, path: Option<PathId>) -> Dynamic {
        self.names().path_name(path)
    }

    /// The path named `name`.
    pub(crate) fn path_named(&self, name: &str) -> Option<PathId> {
        self.names().path_named(name)
    }

    /// The unit type named `name` in the mode's scope: one of the mode's, or an avatar.
    pub(crate) fn unit_type_named(&self, name: &str) -> Option<UnitType> {
        self.names().unit_type_named(name)
    }

    /// The name of `unit_type`, `()` for a unit of no type.
    pub(crate) fn unit_type_name(&self, unit_type: Option<UnitType>) -> Dynamic {
        self.names().unit_type_name(unit_type)
    }

    /// The tag `name`; one the match does not have fails the call.
    pub(crate) fn tag_named(&self, name: &str) -> Result<Tag, ApiError> {
        self.names().tag_named(name)
    }

    /// Every unit, living or dead, that `keep` keeps, by stable id.
    fn units_where(&self, mut keep: impl FnMut(&UnitRow) -> bool) -> Array {
        let rows = self.rows();
        rows.units()
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
        self.rows().row(id).copied()
    }

    /// What `read` reads of the row at `at` among the rows, as a handle names it, in place.
    pub(crate) fn read_row<R>(&self, at: usize, read: impl FnOnce(&UnitRow) -> R) -> R {
        read(&self.rows().units()[at])
    }

    /// Calls `visit` with each row the view read, and its place, in order of stable id.
    pub(crate) fn each_row(&self, mut visit: impl FnMut(usize, &UnitRow)) {
        for (at, row) in self.rows().units().iter().enumerate() {
            visit(at, row);
        }
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
            shape: row.shape,
            tags: row.tags.tags,
        })
    }

    /// The param `name` of `unit_type`.
    pub(crate) fn param_named(&self, unit_type: UnitType, name: &str) -> Option<Dynamic> {
        self.names().param_named(unit_type, name)
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
        let radius = ApiError::radius(radius)?;
        let filter = Filter::parse(filter, self.names().types()).map_err(ApiError::fail)?;
        let rows = self.rows();
        let units = rows.units();
        let query = TargetQuery {
            team: units[of.row_index()].team,
            from: pos,
            shape: Shape::POINT,
            radius,
            filter: &filter,
        };
        let mut targets = self.0.targets.borrow_mut();
        let found = targets.find(&rows, query, seen);
        let unit = |&at: &usize| Dynamic::from(Unit::new(units[at].id, at, self.clone()));
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
        let radius = ApiError::radius(radius)?;
        let filter = Filter::parse(filter, self.names().types()).map_err(ApiError::fail)?;
        let rows = self.rows();
        let of = rows.units()[of.row_index()];
        let query = TargetQuery {
            team: of.team,
            from: of.pos,
            shape: of.shape,
            radius,
            filter: &filter,
        };
        let metric = rows.metric();
        let mut nearest = None;
        self.0
            .targets
            .borrow_mut()
            .visit(&rows, query, seen, |body| {
                let distance = metric.offset(of.pos, body.at).length_squared_bits();
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
            &[&["of", "pos", "radius", "filter"]],
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
    use crate::units::view_names::ViewNames;
    use crate::values::filter_data::FilterData;
    use crate::values::stat::Stat;

    impl View {
        /// The run-time form of `filter`, its tag among those of the match's unit types.
        pub(crate) fn resolve_filter(&self, filter: &FilterData) -> Result<Filter, ApiError> {
            Filter::resolve(filter, self.names().types())
        }

        /// How many unit types the match loaded, for a test to name the next one.
        pub(crate) fn types_count(&self) -> usize {
            self.names().types().count()
        }

        /// Gives scripts the names of the unit types as the view holds them.
        pub(crate) fn share_type_names(&self) {
            self.0.names.borrow_mut().share_type_names();
        }

        pub(crate) fn types_mut(&self) -> RefMut<'_, UnitTypes> {
            RefMut::map(self.0.names.borrow_mut(), ViewNames::types_mut)
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
