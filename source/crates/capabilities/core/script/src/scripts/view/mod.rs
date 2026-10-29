use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_geometry::{Bounds, Metric, Shape};
use campfire_math::{Num, Vec3};
use campfire_names::Filter;
use campfire_names::MatchNames;
use campfire_sim::{MatchRate, StableId};
use campfire_values::{Relation, Tag, Team};
use campfire_visibility::{DataKind, Recipients, Visibility};

use crate::script_host::call_charge::CallCharge;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::view::call_sight::CallSight;
use crate::scripts::view::filter_texts::FilterTexts;
use crate::scripts::view::listing::Listing;
use crate::scripts::view::living_unit::LivingUnit;
use crate::scripts::view::target_index::{TargetIndex, TargetQuery};
use crate::scripts::view::unit::Unit;
use crate::scripts::view::unit_row::UnitRow;
use crate::scripts::view::view_column::{ViewColumn, ViewColumns};
use crate::scripts::view::view_names::ViewNames;
use crate::scripts::view::view_rows::ViewRows;

pub(crate) mod call_sight;
pub(crate) mod filter_texts;
pub(crate) mod kept_rows;
pub(crate) mod listing;
pub(crate) mod living_unit;
pub(crate) mod row_fill;
pub(crate) mod row_reads;
pub(crate) mod target_index;
pub(crate) mod unit;
pub(crate) mod unit_row;
pub(crate) mod view_column;
pub(crate) mod view_names;
pub(crate) mod view_rows;

/// What scripts see, as the host and every handle share it: the match's names, its units, those
/// with a team, as the running phase of the tick began, and the targets' bodies the queries that
/// reach by distance search; and who reads it in the running call, by the session's visibility.
/// The units are read again before each phase that runs scripts, and every call of the phase sees
/// them as they were read. Each part has a cell of its own, so a name lookup does not borrow the
/// rows. Every handle to a unit comes from the view, which makes one only of a unit its viewer
/// sees, and a handle lives only within the call that made it, so it reads as that call's viewer.
#[derive(Clone)]
pub struct View(Rc<ViewFields>);

/// The fields of the view.
#[derive(Debug)]
struct ViewFields {
    tick_rate: MatchRate,
    names: RefCell<ViewNames>,
    /// The revision of the match's `MatchNames` that `names` derive from; none before the first.
    names_at: Cell<Option<u64>>,
    rows: RefCell<ViewRows>,
    targets: RefCell<TargetIndex>,
    /// The filters scripts gave as text, as the names resolve them.
    filters: RefCell<FilterTexts>,
    /// What reads a world's units into the rows, which the module that holds the units gives.
    read: fn(&mut World),
    /// Who reads the view in the running call; the match between calls.
    sight: Cell<CallSight>,
    /// The session's visibility, which says what a player's viewer receives of each unit.
    visibility: Visibility,
    /// The steps the running call's lists visited, which the host counts.
    charge: CallCharge,
    /// The texts of each enum's members, by its declaration's members, made at its first read.
    members: RefCell<Vec<MemberTexts>>,
}

/// An enum's members, and their texts as scripts read them.
#[derive(Debug)]
struct MemberTexts {
    members: Arc<[Arc<str>]>,
    texts: Box<[ScriptText]>,
}

impl View {
    /// The view of a match at `rate` of a session of `visibility`, whose units `read` reads into
    /// its rows, with no names until the load and the mode give them.
    pub fn new(rate: MatchRate, visibility: Visibility, read: fn(&mut World)) -> Self {
        Self(Rc::new(ViewFields {
            tick_rate: rate,
            names: RefCell::default(),
            names_at: Cell::new(None),
            rows: RefCell::default(),
            targets: RefCell::default(),
            filters: RefCell::default(),
            read,
            sight: Cell::new(CallSight::Match),
            visibility,
            charge: CallCharge::default(),
            members: RefCell::default(),
        }))
    }

    /// The text of member `at` of the enum of `members`, made once for the match.
    pub(crate) fn member_text(&self, members: &Arc<[Arc<str>]>, at: u8) -> ScriptText {
        let mut known = self.0.members.borrow_mut();
        let found = known
            .iter()
            .position(|known| Arc::ptr_eq(&known.members, members));
        let place = found.unwrap_or_else(|| {
            let texts = members.iter().map(|member| ScriptText::from(&**member));
            known.push(MemberTexts {
                members: Arc::clone(members),
                texts: texts.collect(),
            });
            known.len() - 1
        });
        known[place].texts[usize::from(at)].clone()
    }

    /// The steps of the running call's lists, a step for each row a list visits.
    pub(crate) fn charge(&self) -> &CallCharge {
        &self.0.charge
    }

    /// Who reads the view in the running call.
    pub(crate) fn sight(&self) -> CallSight {
        self.0.sight.get()
    }

    /// Makes `sight` read the view, for the call that begins.
    pub(crate) fn set_sight(&self, sight: CallSight) {
        self.0.sight.set(sight);
    }

    /// Whether the viewer sees the unit of row `at`: the match sees every unit, and a player those
    /// its team sees.
    fn sees(&self, rows: &ViewRows, at: usize) -> bool {
        match self.sight() {
            CallSight::Match => true,
            CallSight::Player(view) => rows.columns().sees(at, view.team),
        }
    }

    /// Fails a read of the state of `kind` of the unit of row `at` that the viewer does not
    /// receive, as the server's data filters send it to the client of a player's player slot.
    pub fn admit_row(&self, at: usize, kind: DataKind) -> Checked<()> {
        let row = self.rows().units()[at];
        self.admit(row.owner, Some(row.team), kind)
    }

    /// Fails a read of player `player_slot`'s state of `kind` that the viewer does not receive, as
    /// of a unit the player owns.
    pub fn admit_player(&self, player_slot: PlayerSlot, kind: DataKind) -> Checked<()> {
        let team = self.names().teams().and_then(|teams| teams.of(player_slot));
        self.admit(Some(player_slot), team, kind)
    }

    /// Fails a read of the state of `kind` of what `owner` owns, of `team`, that the viewer does
    /// not receive: the match receives all of it; a player its own every kind a client receives,
    /// and another's what the kind's audience gives it, as its ally or its enemy
    /// (10-replication-audiences.md, Each kind has a default audience, and a mode may set it).
    fn admit(&self, owner: Option<PlayerSlot>, team: Option<Team>, kind: DataKind) -> Checked<()> {
        let CallSight::Player(view) = self.sight() else {
            return Ok(());
        };
        let received = match view.recipient(owner, team) {
            Recipients::NONE => kind.is_sent(),
            recipient => self.0.visibility.audience(kind).contains(recipient),
        };
        if !received {
            return Err(ApiError::Unreceived(kind).fail().into());
        }
        Ok(())
    }

    /// Fails a call that asks what `team` sees unless the viewer may: the match may ask of any
    /// team, and a player only of a team that sees as one with its own.
    pub fn admit_sight(&self, team: Team) -> Checked<()> {
        let admitted = match self.sight() {
            CallSight::Match => true,
            CallSight::Player(view) => self.rows().relations().see_as_one(view.team, team),
        };
        if !admitted {
            return Err(ApiError::NotForRole.fail().into());
        }
        Ok(())
    }

    /// The names scripts read, as the last read derived them from the match's `MatchNames`.
    pub fn names(&self) -> Ref<'_, ViewNames> {
        self.0.names.borrow()
    }

    /// The units as the running phase read them.
    pub fn rows(&self) -> Ref<'_, ViewRows> {
        self.0.rows.borrow()
    }

    /// The rows, for the read that fills them.
    pub fn rows_mut(&self) -> RefMut<'_, ViewRows> {
        self.0.rows.borrow_mut()
    }

    /// Reads the units of `world` for the phase that begins, and its names, when its `MatchNames`
    /// changed since the read before.
    pub fn read(&self, world: &mut World) {
        self.read_names(world);
        (self.0.read)(world);
        self.0.targets.borrow_mut().forget();
    }

    /// Derives the names from `world`'s `MatchNames`, when they changed since they last did: the
    /// load and the mode change them, a tick does not.
    fn read_names(&self, world: &World) {
        let Some(names) = world.get_resource::<MatchNames>() else {
            return;
        };
        if self.0.names_at.get() != Some(names.revision()) {
            *self.0.names.borrow_mut() = ViewNames::of(names);
            self.0.filters.borrow_mut().clear();
            self.0.names_at.set(Some(names.revision()));
        }
    }

    /// The filter `text` names, with its tags among the match's, resolved once for each text a
    /// script gives again.
    pub fn filter(&self, text: &ScriptText) -> Result<Filter, ApiError> {
        let names = self.names();
        self.0.filters.borrow_mut().filter(text, names.tags())
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
    pub fn rate(&self) -> MatchRate {
        self.0.tick_rate
    }

    /// How `of` regards `other`, as the units were read.
    pub fn relation(&self, of: Team, other: Team) -> Relation {
        self.rows().relations().between(of, other)
    }

    /// What `read` gives of the column of type `C`; `None` when none was added.
    pub fn column<C: ViewColumn, R>(&self, read: impl FnOnce(&C) -> R) -> Option<R> {
        self.rows().columns().get().map(read)
    }

    /// Changes the column of type `C` by `write`, when one was added.
    /// The next read fills every row again, as the rows may derive from what `write` changes.
    pub fn column_mut<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        let mut rows = self.0.rows.borrow_mut();
        if let Some(column) = rows.column_mut() {
            write(column);
            rows.refill_next();
        }
    }

    /// Makes the next read fill every row. Bevy clamps each change tick older than it compares
    /// exactly as it checks the world's ticks, but not the last read of the view's queries of
    /// changed parts, so after a check those queries may miss a change.
    pub fn refill_next(&self) {
        self.0.rows.borrow_mut().refill_next();
    }

    /// Changes the rows of the column of type `C` by `write`, as a call's effect changes the
    /// units' parts too, which the next read finds changed.
    pub fn write_rows<C: ViewColumn>(&self, write: impl FnOnce(&mut C)) {
        if let Some(column) = self.0.rows.borrow_mut().column_mut() {
            write(column);
        }
    }

    /// The place among the rows of unit `id`, when the view read it.
    pub fn row_index(&self, id: StableId) -> Option<usize> {
        self.rows().index(id)
    }

    /// Every unit, living or dead, that `keep` keeps and the viewer sees, by stable id: a step
    /// of the call for each row.
    pub fn units_where(&self, mut keep: impl FnMut(&UnitRow) -> bool) -> ScriptList {
        let rows = self.rows();
        self.0.charge.add(rows.units().len() as u64);
        rows.units()
            .iter()
            .enumerate()
            .filter(|&(at, row)| keep(row) && self.sees(&rows, at))
            .map(|(at, row)| ScriptValue::of(Unit::new(row.id, at, self.clone())))
            .collect()
    }

    /// Every unit with `tag` that `listing` lists, by stable id.
    pub fn units_tagged(&self, tag: Tag, listing: Listing) -> ScriptList {
        self.units_where(|row| listing.lists(row) && row.tags.tags.contains(tag))
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

    /// The handle of unit `id`, when the view read it and its viewer sees it.
    pub fn unit(&self, id: StableId) -> Option<Unit> {
        let at = self.row_index(id)?;
        self.sees(&self.rows(), at)
            .then(|| Unit::new(id, at, self.clone()))
    }

    /// The handle of unit `id` as a script value: `()` for none, or a unit the view did not read
    /// or its viewer does not see.
    pub fn unit_value(&self, id: Option<StableId>) -> ScriptValue {
        id.and_then(|id| self.unit(id))
            .map_or(ScriptValue::UNIT, ScriptValue::of)
    }

    /// Unit `id`, when it is a living unit that may be a target.
    pub fn living(&self, id: StableId) -> Option<LivingUnit> {
        let row = self.row(id).filter(|row| row.targetable)?;
        Some(LivingUnit {
            id,
            position: row.position,
            team: row.team,
            shape: row.shape,
            tags: row.tags.tags,
        })
    }

    /// The living targets whose bodies come within `radius` of `position` in the map's metric, as
    /// an area of that radius reaches, that `filter` selects relative to `of`, `seen` lets by the
    /// view's columns and their rows, and the viewer sees, by stable id.
    pub fn find(
        &self,
        of: &Unit,
        position: Vec3,
        radius: Num,
        filter: &Filter,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> Checked<ScriptList> {
        debug_assert!(radius >= Num::ZERO, "a script's radius is not negative");
        let rows = self.rows();
        let units = rows.units();
        let query = TargetQuery {
            team: units[of.row_index()?].team,
            from: position,
            shape: Shape::POINT,
            radius,
            filter,
        };
        let mut targets = self.0.targets.borrow_mut();
        let lets = |columns: &ViewColumns, at| seen(columns, at) && self.sees(&rows, at);
        let found = targets.find(&rows, query, (lets, &self.0.charge));
        let unit =
            |&index: &usize| ScriptValue::of(Unit::new(units[index].id, index, self.clone()));
        Ok(found.iter().map(unit).collect())
    }

    /// The nearest living target that `radius` from the edge of `of`'s body reaches in the map's
    /// metric, as a weapon's range does, that `filter` selects relative to it, `seen` lets by the
    /// view's columns and its row, and the viewer sees, by exact distance between centers, the
    /// lower stable id on a tie; `()` when there is none.
    pub fn nearest(
        &self,
        of: &Unit,
        radius: Num,
        filter: &Filter,
        seen: impl Fn(&ViewColumns, usize) -> bool,
    ) -> Checked<ScriptValue> {
        debug_assert!(radius >= Num::ZERO, "a script's radius is not negative");
        let rows = self.rows();
        let of = rows.units()[of.row_index()?];
        let query = TargetQuery {
            team: of.team,
            from: of.position,
            shape: of.shape,
            radius,
            filter,
        };
        let metric = rows.metric();
        let mut nearest = None;
        let lets = |columns: &ViewColumns, at| seen(columns, at) && self.sees(&rows, at);
        self.0
            .targets
            .borrow_mut()
            .visit(&rows, query, (lets, &self.0.charge), |entry| {
                let distance = metric
                    .offset(of.position, entry.body.position)
                    .length_squared_bits();
                let candidate = (distance, entry.id, entry.key);
                if nearest.is_none_or(|best| candidate < best) {
                    nearest = Some(candidate);
                }
            });
        Ok(nearest.map_or(ScriptValue::UNIT, |(_, id, at)| {
            ScriptValue::of(Unit::new(id, at, self.clone()))
        }))
    }
}

/// The view's units change every phase; a handle kept in a failure prints only that it is one.
impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("View")
    }
}
