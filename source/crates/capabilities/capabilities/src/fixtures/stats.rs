use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_common::DeclaredName;
use campfire_declarations::internals::BookLoad;
use campfire_math::Num;
use campfire_sim::StableId;
use campfire_stats::{
    Application, AuraData, ModifierData, NewInstance, PoolBook, PoolData, Reapply, StatBook,
    StatRule, Stats, StatsData,
};
use campfire_units::{UnitTypeCore, Units};
use campfire_values::{EngineStat, ModifierId, Stat, UnitType};

/// The test fixtures of `Stats` that need a crate above its own.
pub trait StatsFixtures {
    /// The rules of the tests' stat book: move speed at most 5, `armor` at least −30, and free
    /// `health`, `health_regen`, `mana`, `attack_speed`, `attack_damage` and turn rate.
    fn rules() -> BTreeMap<Stat, StatRule>;

    /// The tests' pools, by pool id: `health`, which regenerates, then `mana`, which does not.
    fn pools() -> BTreeMap<DeclaredName, PoolData>;

    /// Loads the next type of the mode for each of `types`, and a book of their stats by
    /// `rules`, with a move speed cap of 6, and `pools`; the types, in order.
    fn load_typed_book(world: &mut World, types: &[StatsData]) -> Vec<UnitType>;

    /// A modifier of no stats, tags, params or times that gives `aura`.
    fn modifier_data(aura: Option<AuraData>) -> ModifierData;

    /// An application of the aura `id` of `radius`, one stack with no end, carried by `carrier`.
    fn aura(id: ModifierId, carrier: StableId, radius: Num) -> Application;
}

impl StatsFixtures for Stats {
    fn rules() -> BTreeMap<Stat, StatRule> {
        let free = StatRule::default();
        let speed = StatRule {
            min: None,
            max: Some(Num::int(5)),
        };
        let armor = StatRule {
            min: Some(Num::int(-30)),
            max: None,
        };
        let named = [
            "health",
            "health_regen",
            "mana",
            "mana_share",
            "attack_speed",
            "attack_damage",
        ];
        [
            (Stat::Engine(EngineStat::MoveSpeed), speed),
            (Stat::Engine(EngineStat::TurnRate), free),
            (Stat::named("armor").unwrap(), armor),
        ]
        .into_iter()
        .chain(named.map(|name| (Stat::named(name).unwrap(), free)))
        .collect()
    }

    fn pools() -> BTreeMap<DeclaredName, PoolData> {
        let pool = |max: &str, regen: Option<&str>, share: Option<&str>| PoolData {
            max: Stat::named(max).unwrap(),
            regen: regen.map(|regen| Stat::named(regen).unwrap()),
            regen_share: share.map(|share| Stat::named(share).unwrap()),
        };
        BTreeMap::from([
            (
                DeclaredName::new("health").unwrap(),
                pool("health", Some("health_regen"), None),
            ),
            (
                DeclaredName::new("mana").unwrap(),
                pool("mana", None, Some("mana_share")),
            ),
        ])
    }

    fn load_typed_book(world: &mut World, types: &[StatsData]) -> Vec<UnitType> {
        let mut book = StatBook::new(&Self::rules(), Num::int(6));
        let loaded = types
            .iter()
            .map(|data| {
                let unit_type = Units::load_next_type(world, &UnitTypeCore::default());
                book.add_type(unit_type, data).unwrap();
                unit_type
            })
            .collect();
        let pools = Self::pools();
        let names = Arc::new(pools.keys().map(DeclaredName::as_str).collect());
        let pools = PoolBook::new(&pools, names, None, |_, stat| {
            book.named(stat).ok_or_else(|| stat.clone())
        })
        .unwrap();
        book.load_into(world);
        pools.load_into(world);
        loaded
    }

    fn modifier_data(aura: Option<AuraData>) -> ModifierData {
        ModifierData {
            aura,
            ..ModifierData::default()
        }
    }

    fn aura(id: ModifierId, carrier: StableId, radius: Num) -> Application {
        Application {
            instance: NewInstance {
                aura_radius: Some(radius),
                ..NewInstance::bare(id, Some(carrier))
            },
            reapply: Reapply::Refresh,
            max_stacks: None,
        }
    }
}
