use std::collections::BTreeSet;

use campfire_actions::CostTarget;
use campfire_combat::{Armor, ModeCombatData};
use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, NamedScripts, NamedTexts, StateFields};
use campfire_items::ItemsData;
use campfire_mode::{ChoiceId, MapData, MarkerTag, ModeData, TimerId};
use campfire_navigation::ModeNavigationData;
use campfire_script::{Filter, NameTables};
use campfire_units::{PoolId, TypeParamsData};
use campfire_values::{
    ActionId, ItemId, ModifierId, NameKind, Named, Names, PackageIndex, Param, ResourceId, TextId,
};

/// The names of a match's packages that scripts and data name, each kind of id as the names its
/// module declares, and the names of no id of their own as the modules' sections give them: what
/// the load resolves a script's names by; and what the modules' data names of the scripts, the
/// texts and the state, which the load checks across the packages (02-engine-core.md, A module
/// checks the rules of its own data). A kind of no module the match runs has none.
#[derive(Debug)]
pub struct LoadNames {
    tables: NameTables,
    armors: BTreeSet<DeclaredName>,
    /// The tags of the markers the mode's units spawn at and of the map's markers.
    marker_tags: BTreeSet<DeclaredName>,
    /// The mode's item types, whose ids the item book gives as it builds on the actions.
    items: BTreeSet<DeclaredName>,
    scripts: NamedScripts,
    texts: NamedTexts,
    state_fields: StateFields,
    /// The params a handle holds: each unit type's, each map marker's, and each the mode declares
    /// its maps' markers may hold.
    handle_params: BTreeSet<DeclaredName>,
}

impl LoadNames {
    /// The names of the build `cx`, each built by its owner as the build asks for it; an error
    /// for the first a builder refuses.
    pub(crate) fn of(cx: &mut BookCx<'_>) -> Result<Self, BookError> {
        let tables = cx.name_tables()?;
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
        let items = cx.package(PackageIndex::MODE).content.get::<ItemsData>();
        let items = items.into_iter().flat_map(|items| items.0.keys());
        Ok(Self {
            tables,
            items: items.cloned().collect(),
            scripts: cx.take_fact::<NamedScripts>()?.unwrap_or_default(),
            texts: cx.take_fact::<NamedTexts>()?.unwrap_or_default(),
            state_fields: cx.take_fact::<StateFields>()?.unwrap_or_default(),
            handle_params,
            armors: armors.cloned().collect(),
            marker_tags: spawns.chain(marked).cloned().collect(),
        })
    }

    /// The scripts the packages' data names, each with a role it serves.
    pub const fn scripts(&self) -> &NamedScripts {
        &self.scripts
    }

    /// The texts the packages' data names.
    pub const fn texts(&self) -> &NamedTexts {
        &self.texts
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
        if let Some(has) = self.tables.has(kind, name) {
            return has;
        }
        Some(match kind {
            CostTarget::NAMES => {
                let none = Names::default();
                let pools = self
                    .tables
                    .table::<PoolId>()
                    .map_or(&none, |pools| &**pools);
                let no_resources = Names::default();
                let resources = self.tables.table::<ResourceId>();
                let resources = resources.map_or(&no_resources, |resources| &**resources);
                CostTarget::named(pools, resources, name).is_some()
            }
            Armor::NAMES => self.armors.contains(name),
            MarkerTag::NAMES => self.marker_tags.contains(name),
            ItemId::NAMES => self.items.contains(name),
            ModifierId::NAMES
            | ActionId::NAMES
            | TextId::NAMES
            | Param::NAMES
            | Filter::NAMES
            | ChoiceId::NAMES
            | TimerId::NAMES => return None,
            kind => unreachable!("no module of the release declares the names of every {kind}"),
        })
    }
}
