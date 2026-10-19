use std::fmt;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_common::Seat;
use campfire_values::{
    DamageKind, NameId, NameKey, NameTable, Names, ResourceId, Scalar, Team, UnitType,
};

use crate::scripts::script_names::ScriptNames;
use crate::scripts::type_slots::{Slot, TypeSlots};
use crate::scripts::view::tag_names::TagNames;
use crate::scripts::view::teams::Teams;

/// The names of the match, as its books and its mode give them: one table for each kind of id,
/// found by the id's type; the tags; the teams and their players; and each unit type's params.
/// What a check of the match reads as a plain resource, and what the script view derives the
/// names its scripts read from, as each batch reads it. Package data, not state.
#[derive(Resource, Debug, Default)]
pub struct MatchNames {
    /// The names of each kind of id, a `Table<I>` by the id's type `I`.
    tables: TypeSlots<dyn AnyTable>,
    tags: TagNames,
    /// The match's teams; none before a mode sets them.
    teams: Option<Arc<Teams>>,
    /// Each unit type's params, one run per type, in the order of the types.
    params: Arc<NameTable<Scalar>>,
    /// How many times its names changed, which the script view reads its own again by.
    revision: u64,
}

/// The names of the ids of type `I`.
#[derive(Debug)]
struct Table<I: NameId>(Arc<Names<I>>);

/// A table of the names of one kind of id, as `MatchNames` holds it among the others.
pub(crate) trait AnyTable: Slot + fmt::Debug + Send + Sync {
    /// The table as scripts read it, each name a script string.
    fn script_names(&self) -> Box<dyn AnyNames>;

    fn clone_table(&self) -> Box<dyn AnyTable>;
}

/// A table of the names of one kind of id as scripts read it, among the others.
pub(crate) trait AnyNames: Slot + fmt::Debug {}

impl<I: NameId + 'static> AnyTable for Table<I>
where
    Names<I>: Send + Sync,
{
    fn script_names(&self) -> Box<dyn AnyNames> {
        Box::new(ScriptNames::new(Arc::clone(&self.0)))
    }

    fn clone_table(&self) -> Box<dyn AnyTable> {
        Box::new(Self(Arc::clone(&self.0)))
    }
}

impl<I: NameId + 'static> AnyNames for ScriptNames<I> {}

impl Clone for MatchNames {
    fn clone(&self) -> Self {
        let mut tables = TypeSlots::default();
        for table in &self.tables {
            tables.add(table.clone_table());
        }
        Self {
            tables,
            tags: self.tags.clone(),
            teams: self.teams.clone(),
            params: Arc::clone(&self.params),
            revision: self.revision,
        }
    }
}

impl MatchNames {
    /// Puts in the names of the ids of type `I`, in place of those it held.
    pub fn set_table<I: NameId + 'static>(&mut self, names: Arc<Names<I>>)
    where
        Names<I>: Send + Sync,
    {
        self.revision += 1;
        match self.tables.get_mut::<Table<I>>() {
            Some(held) => *held = Table(names),
            None => {
                self.tables.add(Box::new(Table(names)));
            }
        }
    }

    /// The names of the ids of type `I`; none before their module puts them in.
    pub fn table<I: NameId + 'static>(&self) -> Option<&Names<I>> {
        Some(&self.tables.get::<Table<I>>()?.0)
    }

    /// Each table as scripts read it, in the order the books put them in.
    pub(crate) fn script_tables(&self) -> impl Iterator<Item = Box<dyn AnyNames>> + '_ {
        self.tables.iter().map(AnyTable::script_names)
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

    /// Whether `seat` is a player of the session; any slot is before a mode sets the teams.
    pub fn has_player(&self, seat: Seat) -> bool {
        self.teams().is_none_or(|teams| teams.of(seat).is_some())
    }

    /// Whether `seat` is a player of `team`; any slot is of any team before a mode sets the
    /// teams.
    pub fn plays_for(&self, seat: Seat, team: Team) -> bool {
        self.teams()
            .is_none_or(|teams| teams.of(seat) == Some(team))
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
    use campfire_sim::TickRate;

    use super::*;
    use crate::scripts::script_text::ScriptText;
    use crate::scripts::view::View;

    #[test]
    fn the_view_derives_its_names_from_the_match_names_as_it_reads_once_they_changed() {
        let mut world = World::new();
        let mut names = MatchNames::default();
        names.set_table(Arc::new(Names::<ResourceId>::from(["gold"])));
        world.insert_resource(names);
        let view = View::new(TickRate::new(NonZeroU32::new(30).unwrap()), |_| {});
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
