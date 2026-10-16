use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use campfire_common::PlayerSlot;
use campfire_values::{
    DamageKind, NameId, NameKey, NameTable, Names, ResourceId, Scalar, Team, UnitType,
};

use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_names::ScriptNames;
use crate::scripts::script_text::ScriptText;
use crate::scripts::type_slots::{Slot, TypeSlots};
use crate::scripts::view::tag_names::TagNames;
use crate::scripts::view::teams::Teams;

/// The names scripts read and write, as the match's load and its mode set them: one table for
/// each kind of id, found by the id's type; the tags; the teams and their players; and each unit
/// type's params.
#[derive(Debug, Default)]
pub struct ViewNames {
    /// The names of each kind of id, a `ScriptNames<I>` by the id's type `I`.
    tables: TypeSlots<dyn AnyNames>,
    tags: TagNames,
    /// The match's teams; none before a mode sets them.
    teams: Option<Rc<Teams>>,
    /// Each unit type's params, one run per type, in the order of the types.
    params: Arc<NameTable<Scalar>>,
}

impl ViewNames {
    /// Puts in the names of the ids of type `I`, in place of those it held.
    pub fn set_table<I: NameId + 'static>(&mut self, names: Arc<Names<I>>) {
        let table = ScriptNames::new(names);
        match self.tables.get_mut::<ScriptNames<I>>() {
            Some(held) => *held = table,
            None => {
                self.tables.add(Box::new(table));
            }
        }
    }

    /// The names of the ids of type `I`; none before their owner puts them in.
    pub fn table<I: NameId + 'static>(&self) -> Option<&ScriptNames<I>> {
        self.tables.get::<ScriptNames<I>>()
    }

    /// The id of type `I` named `key`.
    pub fn named<I: NameId + 'static>(&self, key: impl NameKey<I::Scope>) -> Option<I> {
        self.table::<I>()?.named(key)
    }

    /// The name of `id`, an id of the match: its owner put in the table of its kind before it
    /// made any id of it.
    pub fn name<I: NameId + 'static>(&self, id: I) -> ScriptText {
        let table = self
            .table::<I>()
            .expect("the match names each kind of id it holds");
        table.name(id).expect("an id of the match is in its table")
    }

    /// How many ids of type `I` the table names; none before their owner puts it in.
    pub fn count<I: NameId + 'static>(&self) -> usize {
        self.table::<I>().map_or(0, |names| names.names().len())
    }

    /// The match's tags.
    pub const fn tags(&self) -> &TagNames {
        &self.tags
    }

    /// Sets the match's tags, as the load declared them.
    pub fn set_tags(&mut self, tags: TagNames) {
        self.tags = tags;
    }

    /// Sets each unit type's params, one run per type, as the load gives them.
    pub fn set_params(&mut self, params: Arc<NameTable<Scalar>>) {
        self.params = params;
    }

    /// The param `name` of `unit_type`, if its type declares one.
    pub fn param(&self, unit_type: UnitType, name: &str) -> Option<Scalar> {
        let run = unit_type.index();
        let at = self.params.named(run, name)?;
        Some(self.params.values(run)[at])
    }

    /// Names the teams.
    pub fn set_teams(&mut self, teams: Rc<Teams>) {
        self.set_table(Arc::clone(teams.names()));
        self.teams = Some(teams);
    }

    /// Whether `kind` is one of the mode's damage kinds; any is, before a mode names them.
    pub fn has_damage_kind(&self, kind: DamageKind) -> bool {
        let kinds = self.table::<DamageKind>().map(ScriptNames::names);
        kinds.is_none_or(|kinds| kinds.is_empty() || kinds.has(kind))
    }

    /// Whether `team` is one of the mode's teams; any team is before a mode sets them, as every
    /// mode has one at the least.
    pub fn has_team(&self, team: Team) -> bool {
        let teams = self.teams.as_deref();
        teams.is_none_or(|teams| team.index() < teams.count())
    }

    /// Whether `slot` is a player of the session; any slot is before a mode sets the teams.
    pub fn has_player(&self, slot: PlayerSlot) -> bool {
        let teams = self.teams.as_deref();
        teams.is_none_or(|teams| teams.of(slot).is_some())
    }

    /// Whether `slot` is a player of `team`; any slot is of any team before a mode sets the
    /// teams.
    pub fn plays_for(&self, slot: PlayerSlot, team: Team) -> bool {
        let teams = self.teams.as_deref();
        teams.is_none_or(|teams| teams.of(slot) == Some(team))
    }

    /// Whether a player's `resources` amounts make one of each resource the mode declares; any do
    /// before a mode sets the teams.
    pub fn fits_resources(&self, resources: usize) -> bool {
        self.teams.is_none() || resources == self.count::<ResourceId>()
    }

    /// Player `player`'s slot, as a script names it, when the session has it.
    pub(crate) fn player(&self, player: i64) -> Checked<PlayerSlot> {
        match self.teams.as_deref() {
            Some(teams) => teams.player(player),
            None => Err(ApiError::UnknownPlayer.fail().into()),
        }
    }
}

/// A table of the names of one kind of id, as `ViewNames` holds it among the others.
trait AnyNames: Slot + fmt::Debug {}

impl<I: NameId + 'static> AnyNames for ScriptNames<I> {}
