use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::PlayerSlot;
use campfire_values::{
    Allegiance, DamageKind, MatchScope, NameId, NameKey, NameTable, Named, Names, ResourceId,
    Scalar, Team, UnitType,
};

use crate::scripts::view::name_tables::{AnyNames, NameTables};
use crate::scripts::view::tag_names::TagNames;
use crate::scripts::view::teams::Teams;

/// The names of the match, as its books and its mode give them: one table for each kind of id,
/// found by the id's type; the tags; the teams and their players; and each unit type's params.
/// What a check of the match reads as a plain resource, and what the script view derives the
/// names its scripts read from, as each batch reads it. Package data, not state.
#[derive(Resource, Debug, Default, Clone)]
pub struct MatchNames {
    tables: NameTables,
    tags: TagNames,
    /// The match's teams; none before a mode sets them.
    teams: Option<Arc<Teams>>,
    /// Each unit type's params, one run per type, in the order of the types.
    params: Arc<NameTable<Scalar>>,
    /// How many times its names changed, which the script view reads its own again by.
    revision: u64,
}

impl MatchNames {
    /// Puts in the names of the ids of type `I`, in place of those it held.
    pub fn set_table<I: NameId + Named + 'static>(&mut self, names: Arc<Names<I>>)
    where
        Names<I>: Send + Sync,
        I::Scope: MatchScope,
    {
        self.revision += 1;
        self.tables.set(names);
    }

    /// The names of the ids of type `I`; none before their module puts them in.
    pub fn table<I: NameId + 'static>(&self) -> Option<&Names<I>> {
        self.tables.table::<I>().map(|names| &**names)
    }

    /// The tables of the names of each kind of id.
    pub const fn tables(&self) -> &NameTables {
        &self.tables
    }

    /// Each table as scripts read it, in the order the books put them in.
    pub(crate) fn script_tables(&self) -> impl Iterator<Item = Box<dyn AnyNames>> + '_ {
        self.tables.script_tables()
    }

    /// The id of type `I` named `key`.
    pub fn named<I: NameId + 'static>(&self, key: impl NameKey<I::Scope>) -> Option<I> {
        self.table::<I>()?.named(key)
    }

    /// How many ids of type `I` the match names; none before their module puts it in.
    pub fn count<I: NameId + 'static>(&self) -> usize {
        self.table::<I>().map_or(0, Names::len)
    }

    /// The match's tags.
    pub const fn tags(&self) -> &TagNames {
        &self.tags
    }

    /// Sets the match's tags, as the load declared them.
    pub fn set_tags(&mut self, tags: TagNames) {
        self.revision += 1;
        self.tables.set(Arc::clone(tags.names()));
        self.tags = tags;
    }

    /// Sets each unit type's params, one run per type, as the load gives them.
    pub fn set_params(&mut self, params: Arc<NameTable<Scalar>>) {
        self.revision += 1;
        self.params = params;
    }

    /// The param `name` of `unit_type`, if its type declares one.
    pub fn param(&self, unit_type: UnitType, name: &str) -> Option<Scalar> {
        let run = unit_type.index();
        let at = self.params.named(run, name)?;
        Some(self.params.values(run)[at])
    }

    /// Names the teams.
    pub fn set_teams(&mut self, teams: Arc<Teams>) {
        self.set_table(Arc::clone(teams.names()));
        self.teams = Some(teams);
    }

    /// How many times its names changed.
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    /// The match's teams; none before a mode sets them.
    pub fn teams(&self) -> Option<&Teams> {
        self.teams.as_deref()
    }

    /// Whether `kind` is one of the mode's damage kinds; any is, before a mode names them.
    pub fn has_damage_kind(&self, kind: DamageKind) -> bool {
        let kinds = self.table::<DamageKind>();
        kinds.is_none_or(|kinds| kinds.is_empty() || kinds.has(kind))
    }

    /// Whether `team` is one of the mode's teams; any team is before a mode sets them, as every
    /// mode has one at the least.
    pub fn has_team(&self, team: Team) -> bool {
        self.teams()
            .is_none_or(|teams| team.index() < teams.count())
    }

    /// Whether `player_slot` is a player of the session; any slot is before a mode sets the teams.
    pub fn has_player(&self, player_slot: PlayerSlot) -> bool {
        self.teams()
            .is_none_or(|teams| teams.of(player_slot).is_some())
    }

    /// Whether `player_slot` is a player of `team`; any slot is of any team before a mode sets the
    /// teams.
    pub fn plays_for(&self, player_slot: PlayerSlot, team: Team) -> bool {
        self.teams()
            .is_none_or(|teams| teams.of(player_slot) == Some(team))
    }

    /// Whether `allegiance` is one of the match's: a team of the mode's, and an owner who plays
    /// for it.
    pub fn has_allegiance(&self, allegiance: Allegiance) -> bool {
        self.has_team(allegiance.team)
            && allegiance
                .owner
                .is_none_or(|player_slot| self.plays_for(player_slot, allegiance.team))
    }

    /// Whether a player's `resources` amounts make one of each resource the mode declares; any do
    /// before a mode sets the teams.
    pub fn fits_resources(&self, resources: usize) -> bool {
        self.teams.is_none() || resources == self.count::<ResourceId>()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_values::FilterData;

    use crate::scripts::error::UnknownName;
    use crate::scripts::view::filter::Filter;
    use crate::scripts::view::match_names::MatchNames;

    impl MatchNames {
        /// The run-time form of `filter`, its tag among those of the match's unit types.
        pub fn resolve_filter(&self, filter: &FilterData) -> Result<Filter, UnknownName> {
            Filter::resolve(filter, self.tags())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use bevy_ecs::world::World;
    use campfire_sim::MatchRate;

    use super::*;
    use crate::scripts::script_text::ScriptText;
    use crate::scripts::view::View;
    use campfire_visibility::Visibility;

    #[test]
    fn the_view_derives_its_names_from_the_match_names_as_it_reads_once_they_changed() {
        let mut world = World::new();
        let mut names = MatchNames::default();
        names.set_table(Arc::new(Names::<ResourceId>::from(["gold"])));
        world.insert_resource(names);
        let view = View::new(
            MatchRate::new(NonZeroU32::new(30).unwrap()),
            Visibility::DEFAULT,
            |_| {},
        );
        // Before its first read the view holds no name; the read derives them, scripts' strings
        // among them.
        assert_eq!(view.names().count::<ResourceId>(), 0);
        view.read(&mut world);
        let gold = ResourceId::new(0);
        assert_eq!(view.names().named::<ResourceId>("gold"), Some(gold));
        assert_eq!(ScriptText::from(view.names().name(gold)).as_str(), "gold");
        // A change of the match's names reaches the view at its next read, not before.
        let both = Names::<ResourceId>::from(["gold", "wood"]);
        world.resource_mut::<MatchNames>().set_table(Arc::new(both));
        assert_eq!(view.names().count::<ResourceId>(), 1);
        view.read(&mut world);
        assert_eq!(view.names().count::<ResourceId>(), 2);
        assert!(world.resource::<MatchNames>().fits_resources(5));
    }
}
