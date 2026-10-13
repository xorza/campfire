use campfire_actions::{CostTarget, SlotKind};
use campfire_capabilities::Release;
use campfire_core::{Layer, NameKind, PathId, ResourceId, Tag, Team, TrackId, TypeScope, UnitType};
use campfire_stats::PoolId;
use campfire_values::{DamageKind, DeclaredName, NameId, Names};

use crate::mode_packages::ModePackages;

/// The names the mode's packages declare, each kind in the table a match builds of it: what the
/// load check resolves a name of data or a script by. They are built before the checks that
/// refuse a name twice or a kind past its limit, so each table keeps the first id of a name and
/// none past the limit, which those checks then refuse.
#[derive(Debug)]
pub(crate) struct NameTables<'a> {
    resources: Names<ResourceId>,
    pools: Names<PoolId>,
    damage_kinds: Names<DamageKind>,
    tracks: Names<TrackId>,
    /// The slot kinds' own table, which their data builds as it reads.
    slot_kinds: &'a Names<SlotKind>,
    layers: Names<Layer>,
    /// The engine's tags, then every tag the packages name.
    tags: Names<Tag>,
    teams: Names<Team>,
    paths: Names<PathId>,
    /// Each package's unit types in its scope, the mode's and the avatars in the mode's.
    unit_types: Names<UnitType>,
}

impl<'a> NameTables<'a> {
    pub(crate) fn of(packages: &'a ModePackages) -> NameTables<'a> {
        let data = &packages.data;
        let engine = Release::engine_tags();
        let tags = engine.iter().map(|tag| tag.name());
        let declared = packages.tag_names().iter().map(DeclaredName::as_str);
        let own = packages.content.units.keys();
        let own = own.map(|name| (TypeScope::Mode, name.as_str()));
        let avatars = packages.avatar_names().map(|name| (TypeScope::Mode, name));
        let dependencies = (1..)
            .zip(&packages.dependencies)
            .flat_map(|(index, dependent)| {
                let units = dependent.content.units.keys();
                units.map(move |name| (TypeScope::Package(index), name.as_str()))
            });
        NameTables {
            resources: first_of(&data.resources),
            pools: first_of(data.pools.keys()),
            damage_kinds: first_of(&data.combat.damage_kinds),
            tracks: first_of(data.tracks.keys()),
            slot_kinds: data.slots.names(),
            layers: first_of(&data.navigation.layers),
            tags: Names::first_of(tags.chain(declared)),
            teams: first_of(packages.manifest.teams.iter().map(|team| &team.name)),
            paths: first_of(packages.map.paths.iter().map(|path| &path.name)),
            unit_types: Names::first_of(own.chain(avatars).chain(dependencies)),
        }
    }

    /// Whether the mode declares `text` a name of `kind`: a unit type of the mode's scope. A kind
    /// no table holds is the caller's to check.
    pub(crate) fn has(&self, kind: NameKind, text: &str) -> bool {
        match kind {
            NameKind::Resource => self.resources.named(text).is_some(),
            NameKind::Pool => self.pools.named(text).is_some(),
            NameKind::DamageKind => self.damage_kinds.named(text).is_some(),
            NameKind::Track => self.tracks.named(text).is_some(),
            NameKind::SlotKind => self.slot_kinds.named(text).is_some(),
            NameKind::Layer => self.layers.named(text).is_some(),
            NameKind::Tag => self.tags.named(text).is_some(),
            NameKind::Team => self.teams.named(text).is_some(),
            NameKind::Path => self.paths.named(text).is_some(),
            NameKind::UnitType => self.unit_types.named((TypeScope::Mode, text)).is_some(),
            NameKind::Modifier
            | NameKind::Ability
            | NameKind::Filter
            | NameKind::Stat
            | NameKind::Choice
            | NameKind::MarkerTag
            | NameKind::Param
            | NameKind::Cost
            | NameKind::Message
            | NameKind::Item => unreachable!("no table holds the names of a {kind}"),
        }
    }

    pub(crate) const fn resources(&self) -> &Names<ResourceId> {
        &self.resources
    }

    /// What a cost named `name` takes from: a pool, or else a player resource.
    pub(crate) fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
        CostTarget::named(&self.pools, &self.resources, name.as_str())
    }
}

/// The table of `names`, each a name of the first that names it, as `Names::first_of` builds it.
fn first_of<'n, I: NameId<Scope = ()>>(
    names: impl IntoIterator<Item = &'n DeclaredName>,
) -> Names<I> {
    Names::first_of(names.into_iter().map(DeclaredName::as_str))
}
