use std::sync::Arc;

use bevy_ecs::world::World;

use crate::actions::actions_column::ActionsColumn;
use crate::actions::slot_kinds::SlotKinds;
use crate::combat::combat_bindings::CombatBindings;
use crate::items::shop::Shop;
use crate::mode::mode_data::ModeData;
use crate::mode::mode_map::ModeMap;
use crate::mode::mode_setup::UnitTypeSetup;
use crate::navigation::walker::Walker;
use crate::stats::Stats;
use crate::stats::life_pool::LifePool;
use crate::stats::pool_book::PoolBook;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_book::StatBook;
use crate::units::body::BodyForm;
use crate::units::by_type::ByType;
use crate::units::layer::Layer;
use crate::units::script_view::View;
use crate::units::tag_book::TagBook;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;

/// The books of the mode's own rules: its stats, at the match's rate, under its move speed cap;
/// its pools; its tags' properties and each unit type's own tags; what its combat reads; its slot
/// kinds; the names of its damage kinds and of its players' resources, by id; and its shop.
#[derive(Debug)]
pub struct ModeBooks {
    pub(crate) stats: StatBook,
    pub(crate) pools: PoolBook,
    pub(crate) tags: TagBook,
    /// None with no life pool, as in a mode with no combat.
    pub(crate) life: Option<LifePool>,
    pub(crate) bindings: Option<CombatBindings>,
    pub(crate) slot_kinds: SlotKinds,
    pub(crate) damage_kinds: Arc<[DeclaredName]>,
    pub(crate) resources: Arc<[DeclaredName]>,
    pub(crate) map: ModeMap,
    /// Its shop, when it has one.
    pub(crate) shop: Option<Shop>,
    /// Each kind of unit that walks, by its layer and its body's radius, in order, each once: the
    /// clearances of the map's pathing grid.
    pub(crate) walkers: Vec<Walker>,
    /// The ranks of every loadout entry.
    pub(crate) loadout_ranks: u8,
}

/// What the mode's books are built of: its `data`, which the package load checked; its unit
/// types that stand, `unit_types`, among all of the match's `types`; the walker of each type that
/// walks; and what the book builder derived of them.
#[derive(Debug)]
pub(crate) struct ModeBooksInput<'a> {
    pub(crate) data: &'a ModeData,
    pub(crate) unit_types: &'a [UnitTypeSetup],
    pub(crate) types: &'a mut UnitTypes,
    pub(crate) walkers: &'a ByType<Walker>,
    pub(crate) stats: StatBook,
    /// The life pool, none when the mode names none.
    pub(crate) life: Option<PoolId>,
    /// The ranks of every loadout entry.
    pub(crate) loadout_ranks: u8,
    pub(crate) map: ModeMap,
    pub(crate) shop: Option<Shop>,
}

impl ModeBooks {
    /// Puts the books in `world`, a match whose capabilities are installed: the stat and pool
    /// books, the tags' properties, what combat reads, the slot kinds, and the names its scripts
    /// read. `Mode::install` takes `walkers` and `loadout_ranks` first, calls it, and installs
    /// the map it gives back for the kinds of `walkers`; a test arena calls it alone, for a match
    /// whose mode runs no script and has no map.
    pub fn install(self, world: &mut World) -> ModeMap {
        let ModeBooks {
            stats,
            pools,
            tags,
            life,
            bindings,
            slot_kinds,
            damage_kinds,
            resources,
            map,
            shop,
            walkers: _,
            loadout_ranks: _,
        } = self;
        world
            .non_send::<View>()
            .set_mode_names(&damage_kinds, resources);
        if let Some(life) = life {
            world.insert_resource(life);
        }
        if let Some(bindings) = bindings {
            world.insert_resource(bindings);
        }
        Stats::load(world, stats, pools);
        world.insert_resource(tags);
        ActionsColumn::share_kinds(world.non_send::<View>(), slot_kinds.clone());
        world.insert_resource(slot_kinds);
        if let Some(shop) = shop {
            world.insert_resource(shop);
        }
        map
    }

    /// The books of `input`. Each unit type is tagged with the name of the layer it moves on,
    /// among `input.types`, when the mode names its layers.
    pub(crate) fn build(input: ModeBooksInput<'_>) -> ModeBooks {
        let ModeBooksInput {
            data,
            unit_types,
            types,
            walkers,
            stats,
            life,
            loadout_ranks,
            map,
            shop,
        } = input;
        let life = life.map(LifePool);
        let layers = &data.navigation.layers;
        for unit_type in unit_types {
            let layer = unit_type.kit.body.map_or(Layer::FIRST, BodyForm::layer);
            if let Some(name) = layers.get(usize::from(layer.index())) {
                let tag = types
                    .tag_named(name.as_str())
                    .expect("the match declared every tag its packages name");
                types.give_tag(unit_type.unit_type, tag);
            }
        }
        ModeBooks {
            pools: PoolBook::new(&data.pools, &stats),
            bindings: life.map(|_| CombatBindings::new(&data.combat, &stats)),
            life,
            tags: types.tag_book(&data.tags),
            stats,
            slot_kinds: data.slots.clone(),
            damage_kinds: data.combat.damage_kinds.as_slice().into(),
            resources: data.resources.as_slice().into(),
            map,
            shop,
            loadout_ranks,
            walkers: {
                let mut kinds: Vec<Walker> = walkers.values().copied().collect();
                kinds.sort_unstable();
                kinds.dedup();
                kinds
            },
        }
    }
}
