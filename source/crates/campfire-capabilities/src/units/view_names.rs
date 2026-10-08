use std::rc::Rc;
use std::sync::Arc;

use campfire_common::PlayerSlot;
use campfire_script::rhai::{Dynamic, INT, ImmutableString};

use crate::players::resource_id::ResourceId;
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_consts::ScriptConsts;
use crate::units::action_id::ActionId;
use crate::units::path_id::PathId;
use crate::units::tag::Tag;
use crate::units::team::Team;
use crate::units::teams::Teams;
use crate::units::track_id::TrackId;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type::UnitType;
use crate::units::unit_types::UnitTypes;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::name_list::NameList;

/// The names scripts read and write, as the match's load and its mode set them: the unit types
/// and tags, the teams and players, the paths, the damage kinds, tracks and actions, and the
/// players' resources.
#[derive(Debug, Default)]
pub(crate) struct ViewNames {
    types: UnitTypes,
    /// The match's teams; none before a mode sets them.
    teams: Option<Rc<Teams>>,
    /// The name of each path, by index, once a mode sets them.
    paths: Arc<NameList>,
    /// The names scripts read, as they read them.
    consts: ScriptConsts,
    /// The players' resources the mode declares, by resource id.
    resource_names: Arc<[DeclaredName]>,
}

impl ViewNames {
    pub(crate) const fn types(&self) -> &UnitTypes {
        &self.types
    }

    /// Sets the match's unit types and tags, as the load built them, and gives scripts their
    /// names.
    pub(crate) fn set_types(&mut self, types: UnitTypes) {
        self.types = types;
        self.share_type_names();
    }

    /// Gives scripts the names of the unit types as the view holds them.
    pub(crate) fn share_type_names(&mut self) {
        self.consts.set_unit_types(self.types.names());
    }

    /// Names the teams and the paths.
    pub(crate) fn set_teams(&mut self, teams: Rc<Teams>, paths: Arc<NameList>) {
        self.teams = Some(teams);
        self.paths = paths;
    }

    /// Sets the damage kinds and the players' resources the mode declares, by id.
    pub(crate) fn set_mode_names(
        &mut self,
        damage_kinds: &[DeclaredName],
        resources: Arc<[DeclaredName]>,
    ) {
        let kinds = damage_kinds.iter().map(DeclaredName::as_str);
        self.consts.set_damage_kinds(kinds);
        self.resource_names = resources;
    }

    /// Names the tracks the mode declares, by track id.
    pub(crate) fn set_track_names<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        self.consts.set_tracks(names);
    }

    /// Names the match's actions, by action id.
    pub(crate) fn set_action_names<'a>(&mut self, names: impl Iterator<Item = &'a str>) {
        self.consts.set_actions(names);
    }

    /// Whether the match loaded `unit_type`.
    pub(crate) fn has_type(&self, unit_type: UnitType) -> bool {
        self.types.contains(unit_type)
    }

    /// Whether `kind` is one of the mode's damage kinds; any is, before a mode names them.
    pub(crate) fn has_damage_kind(&self, kind: DamageKind) -> bool {
        self.consts.has_damage_kind(kind)
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

    /// Whether amounts of `resources` resources in `amounts` places make a row of each resource
    /// the mode declares for each player; any do before a mode sets the teams.
    pub(crate) fn fits_resources(&self, resources: usize, amounts: usize) -> bool {
        self.teams.as_deref().is_none_or(|teams| {
            let players = teams.players() as usize;
            resources == self.resource_names.len()
                && Some(amounts) == players.checked_mul(resources)
        })
    }

    /// Player `player`'s slot, as a script names it, when the session has it.
    pub(crate) fn player(&self, player: INT) -> Checked<PlayerSlot> {
        match self.teams.as_deref() {
            Some(teams) => teams.player(player),
            None => Err(ApiError::UnknownPlayer.fail().into()),
        }
    }

    /// The name of `team`; none for a team the mode does not have.
    pub(crate) fn team_name(&self, team: Team) -> Option<ImmutableString> {
        let name = self.teams.as_deref()?.name(team)?;
        Some(ImmutableString::from(name))
    }

    /// The player resource `name`; `None` for one the mode does not declare.
    pub(crate) fn resource_named(&self, name: &str) -> Option<ResourceId> {
        ResourceId::named(&self.resource_names, name)
    }

    /// The damage kind `name`, when the mode declares it.
    pub(crate) fn damage_kind_named(&self, name: &str) -> Option<DamageKind> {
        self.consts.damage_kind_named(name)
    }

    /// The name of track `track`.
    pub(crate) fn track_name(&self, track: TrackId) -> Option<ImmutableString> {
        self.consts.track(track)
    }

    /// The name of damage kind `kind`; none before a mode names the damage kinds.
    pub(crate) fn damage_kind_name(&self, kind: DamageKind) -> Option<ImmutableString> {
        self.consts.damage_kind(kind)
    }

    /// The name of ability `id` in its package.
    pub(crate) fn ability_name(&self, id: ActionId) -> Option<ImmutableString> {
        self.consts.action(id)
    }

    /// The name of `path`.
    pub(crate) fn path_name(&self, path: PathId) -> Option<ImmutableString> {
        self.paths.get(path.index()).map(ImmutableString::from)
    }

    /// The path named `name`.
    pub(crate) fn path_named(&self, name: &str) -> Option<PathId> {
        let at = self.paths.named(name)?;
        Some(PathId::new(u32::try_from(at).expect("paths fit u32")))
    }

    /// The unit type named `name` in the mode's scope: one of the mode's, or an avatar.
    pub(crate) fn unit_type_named(&self, name: &str) -> Option<UnitType> {
        self.types.named(TypeScope::Mode, name)
    }

    /// The name of `unit_type`.
    pub(crate) fn unit_type_name(&self, unit_type: UnitType) -> Option<ImmutableString> {
        self.consts.unit_type(unit_type)
    }

    /// The tag `name`, when the match has it.
    pub(crate) fn tag_named(&self, name: &str) -> Option<Tag> {
        self.types.tag_named(name)
    }

    /// The param `name` of `unit_type`.
    pub(crate) fn param_named(&self, unit_type: UnitType, name: &str) -> Option<Dynamic> {
        let value = self.types.param_named(unit_type, name)?;
        Some(value.to_dynamic())
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
