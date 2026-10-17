use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use campfire_actions::{
    ActionsData, CostTarget, DamageKindBook, ModifierWays, SlotKindBook, TrackNames,
};
use campfire_combat::ModeCombatData;
use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError};
use campfire_mode::MapData;
use campfire_navigation::{ModeNavigationData, NavigationBook};
use campfire_players::ResourceBook;
use campfire_script::NameKind;
use campfire_stats::{ModifiersData, PoolId, PoolNames};
use campfire_units::{LayerNames, TeamNames, UnitTypes};
use campfire_values::{
    DamageKind, Names, PackageIndex, PathId, ResourceId, Team, TrackId, TypeScope,
};

/// The names of a match's packages that scripts and data name, each kind as the book or the fact
/// of the module that owns it builds it: what the load resolves a script's names by
/// (02-engine-core.md, A module checks the rules of its own data). A kind of no module the match
/// runs has none.
#[derive(Debug)]
pub struct LoadNames {
    resources: Arc<Names<ResourceId>>,
    pools: Arc<Names<PoolId>>,
    damage_kinds: Arc<Names<DamageKind>>,
    tracks: Arc<Names<TrackId>>,
    slot_kinds: SlotKindBook,
    layers: LayerNames,
    /// The engine's tags, then every tag the packages name; each package's unit types in its
    /// scope, the mode's and the avatars in the mode's.
    types: UnitTypes,
    teams: Names<Team>,
    paths: Names<PathId>,
    armors: BTreeSet<DeclaredName>,
    /// The tags of the markers the mode's units spawn at and of the map's markers.
    marker_tags: BTreeSet<DeclaredName>,
    /// The params that the actions that apply each package's modifier give it, by the package
    /// and the modifier: what its script may read beside its own.
    given_params: BTreeMap<(PackageIndex, DeclaredName), BTreeSet<DeclaredName>>,
}

impl LoadNames {
    /// The params the actions of each package give each of its modifiers they apply.
    fn given_params_of(
        cx: &BookCx<'_>,
    ) -> Result<BTreeMap<(PackageIndex, DeclaredName), BTreeSet<DeclaredName>>, BookError> {
        let mut given = BTreeMap::new();
        let ways = cx.fact::<ModifierWays>()?;
        for (package, source) in cx.packages() {
            let (Some(modifiers), Some(actions)) = (
                source.content.get::<ModifiersData>(),
                source.content.get::<ActionsData>(),
            ) else {
                continue;
            };
            for id in modifiers.0.keys() {
                let params = ways.given_params(package, id.as_str(), &actions.0);
                given.insert((package, id.clone()), params.cloned().collect());
            }
        }
        Ok(given)
    }

    /// The names of the build `cx`, each built by its owner as the build asks for it; an error
    /// for the first a builder refuses.
    pub(crate) fn of(cx: &mut BookCx<'_>) -> Result<Self, BookError> {
        let resources = cx
            .book_if::<ResourceBook>()?
            .map(|book| Arc::clone(&book.0));
        let pools = cx.fact_if::<PoolNames>()?.map(|names| Arc::clone(&names.0));
        let damage_kinds = cx
            .book_if::<DamageKindBook>()?
            .map(|book| Arc::clone(&book.0));
        let tracks = cx
            .fact_if::<TrackNames>()?
            .map(|names| Arc::clone(&names.0));
        let slot_kinds = SlotKindBook(Arc::clone(&cx.book::<SlotKindBook>()?.0));
        let paths = cx.book_if::<NavigationBook>()?;
        let paths = paths.map(|book| book.paths.names().clone());
        let combat = cx.mode_section::<ModeCombatData>();
        let armors = combat.into_iter().flat_map(|combat| combat.armors.keys());
        let spawns = cx.mode_section::<ModeNavigationData>().into_iter();
        let spawns = spawns.flat_map(|navigation| &navigation.spawn_tags);
        let map = cx
            .given::<MapData>()
            .expect("the load gives the mode's map");
        let marked = map.markers.iter().flat_map(|marker| &marker.tags);
        let given_params = Self::given_params_of(cx)?;
        Ok(Self {
            given_params,
            resources: resources.unwrap_or_default(),
            pools: pools.unwrap_or_default(),
            damage_kinds: damage_kinds.unwrap_or_default(),
            tracks: tracks.unwrap_or_default(),
            slot_kinds,
            layers: cx.take_fact::<LayerNames>()?.unwrap_or_default(),
            armors: armors.cloned().collect(),
            marker_tags: spawns.chain(marked).cloned().collect(),
            paths: paths.unwrap_or_default(),
            teams: cx
                .take_fact::<TeamNames>()?
                .map_or_else(Names::default, |names| names.0),
            types: cx
                .take_fact::<UnitTypes>()?
                .expect("the core declares the unit types"),
        })
    }

    /// The params that the actions that apply modifier `id` of the package at `package` give it,
    /// which its script may read beside its own.
    pub fn given_params(
        &self,
        package: PackageIndex,
        id: &DeclaredName,
    ) -> impl Iterator<Item = &DeclaredName> {
        let given = self.given_params.get(&(package, id.clone()));
        given.into_iter().flatten()
    }

    /// Whether the match has the name `name` of `kind`, a unit type of the mode's scope; none for
    /// a kind whose names are a package's own, as its modifiers and abilities, which the load
    /// reads of the package itself.
    pub fn has(&self, kind: NameKind, name: &str) -> Option<bool> {
        Some(match kind {
            NameKind::Resource => self.resources.named(name).is_some(),
            NameKind::Pool => self.pools.named(name).is_some(),
            NameKind::Cost => CostTarget::named(&self.pools, &self.resources, name).is_some(),
            NameKind::DamageKind => self.damage_kinds.named(name).is_some(),
            NameKind::Track => self.tracks.named(name).is_some(),
            NameKind::SlotKind => self.slot_kinds.0.names().named(name).is_some(),
            NameKind::Layer => self.layers.0.named(name).is_some(),
            NameKind::Tag => self.types.tags().named(name).is_some(),
            NameKind::Team => self.teams.named(name).is_some(),
            NameKind::Path => self.paths.named(name).is_some(),
            NameKind::UnitType => self.types.types().named((TypeScope::Mode, name)).is_some(),
            NameKind::Armor => self.armors.iter().any(|armor| armor.as_str() == name),
            NameKind::MarkerTag => self.marker_tags.iter().any(|tag| tag.as_str() == name),
            NameKind::Modifier
            | NameKind::Ability
            | NameKind::Message
            | NameKind::Param
            | NameKind::Filter
            | NameKind::Stat
            | NameKind::Choice
            | NameKind::Item => return None,
        })
    }
}
