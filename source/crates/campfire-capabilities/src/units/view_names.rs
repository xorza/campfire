use std::rc::Rc;
use std::sync::Arc;

use campfire_common::PlayerSlot;
use campfire_script::rhai::INT;

use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_names::ScriptNames;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::units::path_id::PathId;
use crate::units::resource_id::ResourceId;
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::track_id::TrackId;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::values::damage_kind::DamageKind;
use crate::values::names::Names;

/// The names scripts read and write, as the match's load and its mode set them: the unit types
/// and tags, the teams and players, the paths, the damage kinds, tracks and actions, and the
/// players' resources, each kind in its table.
#[derive(Debug, Default)]
pub(crate) struct ViewNames {
    types: UnitTypes,
    /// The unit types' names, each in its scope, as `types` holds them.
    unit_types: ScriptNames<UnitType>,
    /// The match's teams; none before a mode sets them.
    teams: Option<Rc<Teams>>,
    /// The teams' names, as `teams` holds them.
    team_names: ScriptNames<Team>,
    pub(crate) paths: ScriptNames<PathId>,
    /// The actions' names, each in its package.
    pub(crate) actions: ScriptNames<ActionId>,
    pub(crate) tracks: ScriptNames<TrackId>,
    pub(crate) damage_kinds: ScriptNames<DamageKind>,
    pub(crate) resources: Arc<Names<ResourceId>>,
    /// The modifiers' names, each in its package.
    pub(crate) modifiers: Arc<Names<ModifierId>>,
}

impl ViewNames {
    /// The names of a match whose types and tags are `types`, before the load and the mode set
    /// the rest.
    pub(crate) fn new(types: UnitTypes) -> ViewNames {
        ViewNames {
            types,
            ..ViewNames::default()
        }
    }

    /// The unit types and tags, with each type's params.
    pub(crate) const fn types(&self) -> &UnitTypes {
        &self.types
    }

    /// The unit types' names, each in its scope.
    pub(crate) const fn unit_types(&self) -> &ScriptNames<UnitType> {
        &self.unit_types
    }

    /// The teams' names; none before a mode sets them.
    pub(crate) const fn team_names(&self) -> &ScriptNames<Team> {
        &self.team_names
    }

    /// Sets the match's unit types and tags, as the load built them, and gives scripts their
    /// names.
    pub(crate) fn set_types(&mut self, types: UnitTypes) {
        self.types = types;
        self.share_type_names();
    }

    /// Gives scripts the names of the unit types as the view holds them.
    pub(crate) fn share_type_names(&mut self) {
        self.unit_types = ScriptNames::new(Arc::clone(self.types.types()));
    }

    /// Names the teams.
    pub(crate) fn set_teams(&mut self, teams: Rc<Teams>) {
        self.team_names = ScriptNames::new(Arc::clone(teams.names()));
        self.teams = Some(teams);
    }

    /// Names the paths.
    pub(crate) fn set_paths(&mut self, paths: Arc<Names<PathId>>) {
        self.paths = ScriptNames::new(paths);
    }

    /// Sets the damage kinds and the players' resources the mode declares.
    pub(crate) fn set_mode_names(
        &mut self,
        damage_kinds: Arc<Names<DamageKind>>,
        resources: Arc<Names<ResourceId>>,
    ) {
        self.damage_kinds = ScriptNames::new(damage_kinds);
        self.resources = resources;
    }

    /// Whether `kind` is one of the mode's damage kinds; any is, before a mode names them.
    pub(crate) fn has_damage_kind(&self, kind: DamageKind) -> bool {
        let kinds = self.damage_kinds.names();
        kinds.is_empty() || kinds.has(kind)
    }

    /// Whether `team` is one of the mode's teams; any team is before a mode sets them, as every
    /// mode has one at the least.
    pub(crate) fn has_team(&self, team: Team) -> bool {
        let teams = self.teams.as_deref();
        teams.is_none_or(|teams| team.index() < teams.count())
    }

    /// Whether `slot` is a player of the session; any slot is before a mode sets the teams.
    pub(crate) fn has_player(&self, slot: PlayerSlot) -> bool {
        let teams = self.teams.as_deref();
        teams.is_none_or(|teams| teams.of(slot).is_some())
    }

    /// Whether `slot` is a player of `team`; any slot is of any team before a mode sets the
    /// teams.
    pub(crate) fn plays_for(&self, slot: PlayerSlot, team: Team) -> bool {
        let teams = self.teams.as_deref();
        teams.is_none_or(|teams| teams.of(slot) == Some(team))
    }

    /// Whether a player's `resources` amounts make one of each resource the mode declares; any do
    /// before a mode sets the teams.
    pub(crate) fn fits_resources(&self, resources: usize) -> bool {
        self.teams.is_none() || resources == self.resources.len()
    }

    /// Player `player`'s slot, as a script names it, when the session has it.
    pub(crate) fn player(&self, player: INT) -> Checked<PlayerSlot> {
        match self.teams.as_deref() {
            Some(teams) => teams.player(player),
            None => Err(ApiError::UnknownPlayer.fail().into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::units::unit_types::UnitTypes;
    use crate::units::view_names::ViewNames;

    impl ViewNames {
        pub(crate) const fn types_mut(&mut self) -> &mut UnitTypes {
            &mut self.types
        }
    }
}
