use std::collections::BTreeSet;
use std::sync::Arc;

use campfire_actions::{CostTarget, DamageKindBook, SlotKind, SlotKindBook, TrackNames};
use campfire_combat::{Armor, ModeCombatData};
use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, NamedMessages, NamedScripts, StateFields};
use campfire_items::ItemsData;
use campfire_mode::{ChoiceId, MapData, MarkerTag, ModeData, TimerId};
use campfire_navigation::{ModeNavigationData, NavigationBook};
use campfire_players::ResourceBook;
use campfire_script::Filter;
use campfire_stats::ModeStatsData;
use campfire_units::{
    Layer, LayerNames, PoolId, PoolNames, StatId, TeamNames, TypeParamsData, UnitTypes,
};
use campfire_values::{
    ActionId, DamageKind, ItemId, MessageId, ModifierId, NameKind, Named, Names, PackageIndex,
    Param, PathId, ResourceId, Stat, Tag, Team, TrackId, TypeScope, UnitType,
};

/// The names of a match's packages that scripts and data name, each kind as the book or the fact
/// of the module that owns it builds it: what the load resolves a script's names by; and what the
/// modules' data names of the scripts, the texts and the state, which the load checks across the
/// packages (02-engine-core.md, A module checks the rules of its own data). A kind of no module the
/// match runs has none.
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
    /// The mode's stats.
    stats: BTreeSet<Stat>,
    /// The mode's item types.
    items: BTreeSet<DeclaredName>,
    scripts: NamedScripts,
    messages: NamedMessages,
    state_fields: StateFields,
    /// The params a handle holds: each unit type's, each map marker's, and each the mode declares
    /// its maps' markers may hold.
    handle_params: BTreeSet<DeclaredName>,
}

impl LoadNames {
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
        let type_params = cx
            .types()
            .iter()
            .filter_map(|unit| unit.data.section::<TypeParamsData>());
        let type_params = type_params.flat_map(|params| params.0.keys());
        let marker_params = map.markers.iter().flat_map(|marker| marker.params.keys());
        let declared = cx.given::<ModeData>().into_iter();
        let declared = declared.flat_map(|data| &data.marker_params);
        let handle_params = type_params
            .chain(marker_params)
            .chain(declared)
            .cloned()
            .collect();
        let stats = cx.mode_section::<ModeStatsData>();
        let stats = stats.into_iter().flat_map(|stats| stats.0.keys());
        let items = cx.package(PackageIndex::MODE).content.get::<ItemsData>();
        let items = items.into_iter().flat_map(|items| items.0.keys());
        Ok(Self {
            stats: stats.cloned().collect(),
            items: items.cloned().collect(),
            scripts: cx.take_fact::<NamedScripts>()?.unwrap_or_default(),
            messages: cx.take_fact::<NamedMessages>()?.unwrap_or_default(),
            state_fields: cx.take_fact::<StateFields>()?.unwrap_or_default(),
            handle_params,
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

    /// The scripts the packages' data names, each with a role it serves.
    pub const fn scripts(&self) -> &NamedScripts {
        &self.scripts
    }

    /// The messages the packages' data names.
    pub const fn messages(&self) -> &NamedMessages {
        &self.messages
    }

    /// Whether a unit type or a map marker holds the param `name`, as a script may read it after
    /// a handle's `.params`.
    pub fn has_handle_param(&self, name: &str) -> bool {
        self.handle_params.contains(name)
    }

    /// The state fields the packages' data declares, of a modifier or a unit type.
    pub const fn state_fields(&self) -> &StateFields {
        &self.state_fields
    }

    /// Whether the match has the name `name` of `kind`, a unit type of the mode's scope; none for
    /// a kind whose names are a package's own, as its modifiers and abilities, which the load
    /// reads of the package itself.
    pub fn has(&self, kind: NameKind, name: &str) -> Option<bool> {
        Some(match kind {
            ResourceId::NAMES => self.resources.named(name).is_some(),
            PoolId::NAMES => self.pools.named(name).is_some(),
            CostTarget::NAMES => CostTarget::named(&self.pools, &self.resources, name).is_some(),
            DamageKind::NAMES => self.damage_kinds.named(name).is_some(),
            TrackId::NAMES => self.tracks.named(name).is_some(),
            SlotKind::NAMES => self.slot_kinds.0.names().named(name).is_some(),
            Layer::NAMES => self.layers.0.named(name).is_some(),
            Tag::NAMES => self.types.tags().named(name).is_some(),
            Team::NAMES => self.teams.named(name).is_some(),
            PathId::NAMES => self.paths.named(name).is_some(),
            UnitType::NAMES => self.types.types().named((TypeScope::Mode, name)).is_some(),
            Armor::NAMES => self.armors.iter().any(|armor| armor.as_str() == name),
            MarkerTag::NAMES => self.marker_tags.iter().any(|tag| tag.as_str() == name),
            StatId::NAMES => Stat::named(name).is_some_and(|stat| self.stats.contains(&stat)),
            ItemId::NAMES => self.items.contains(name),
            ModifierId::NAMES
            | ActionId::NAMES
            | MessageId::NAMES
            | Param::NAMES
            | Filter::NAMES
            | ChoiceId::NAMES
            | TimerId::NAMES => return None,
            kind => unreachable!("the load knows the names of every {kind}"),
        })
    }
}
