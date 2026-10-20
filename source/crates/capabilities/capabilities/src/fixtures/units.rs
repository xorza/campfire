use std::sync::Arc;

use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use campfire_actions::CostTarget;
use campfire_common::Tick;
use campfire_script::{Filter, MatchNames, TagNames};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{SimSystems, SimTick, StableId};
use campfire_stats::{CarriedMut, ModifierBook, ModifierData, Modifiers, Stats};
use campfire_units::internals::TEST_PACKAGE;
use campfire_units::{PoolId, StatId, StatusTags, TagBook, Units};
use campfire_values::{
    Block, DamageKind, Names, RelationSet, ResourceId, Stat, TagProperties, UnitType,
};

use crate::release::Release;

/// The test fixtures of `Units` that need a crate above its own.
pub trait UnitsFixtures {
    /// Loads the modifiers that grant the test tags, each of its tag's name, in the package
    /// `TEST_PACKAGE`, after every other, unless the match loaded them.
    fn load_test_modifiers(world: &mut World);

    /// Whether the match loaded the modifiers that grant the test tags.
    fn loads_test_modifiers(world: &World) -> bool;

    /// Holds on unit `id` the test tags whose effects make `properties`, in place of those it
    /// held, each by the modifier that grants it, as a stun's does; and derives its tags at
    /// once, as the stats derive them.
    fn hold_properties(world: &mut World, id: StableId, properties: TagProperties);

    /// Holds on unit `id` the test tags that block `blocks`, in place of those it held, as
    /// `hold_properties` does.
    fn set_blocks(world: &mut World, id: StableId, blocks: &[Block]);

    /// Holds on unit `id` the test tags that block `blocks` in `stage` of tick `tick`, as
    /// `set_blocks` does then, as a modifier applied in that stage would.
    fn block_at(
        world: &mut World,
        id: StableId,
        tick: u64,
        stage: SimSystems,
        blocks: &'static [Block],
    );

    /// Names, for the scripts of a match with no mode, its `damage_kinds`, its `pools` and its
    /// players' `resources`, each by id, as a mode's books name them.
    fn name_kinds(world: &mut World, damage_kinds: &[&str], pools: &[&str], resources: &[&str]);
}

impl UnitsFixtures for Units {
    fn load_test_modifiers(world: &mut World) {
        if Self::loads_test_modifiers(world) {
            return;
        }
        let mut tags = Self::test_tags();
        tags.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        for (name, _) in tags {
            let grants = ModifierData {
                tags: vec![name.clone()],
                ..ModifierData::default()
            };
            Stats::load_modifier(world, TEST_PACKAGE, name.as_str(), &grants, None);
        }
    }

    fn loads_test_modifiers(world: &World) -> bool {
        let (first, _) = &Self::test_tags()[0];
        world.get_resource::<ModifierBook>().is_some()
            && Stats::modifier(world, TEST_PACKAGE, first.as_str()).is_some()
    }

    fn hold_properties(world: &mut World, id: StableId, properties: TagProperties) {
        let blocks = Block::ALL.map(|block| properties.blocks(block));
        let wanted = blocks
            .into_iter()
            .chain([properties.is_hidden(), properties.detects()]);
        let entity = world.unit_entity(id);
        Self::load_test_modifiers(world);
        for ((name, _), wanted) in Self::test_tags().iter().zip(wanted) {
            let modifier = Stats::modifier(world, TEST_PACKAGE, name.as_str()).unwrap();
            let held = world
                .get::<Modifiers>(entity)
                .is_some_and(|modifiers| modifiers.get(modifier, None).is_some());
            if wanted && !held {
                Stats::give_modifier(world, id, modifier, None, false);
            } else if !wanted && held {
                CarriedMut::with(world, entity, |carried| carried.remove(modifier, None)).unwrap();
            }
        }
        let unit = world.entity(entity);
        let unit_type = *unit.get::<UnitType>().unwrap();
        let status = unit.get::<StatusTags>().copied();
        let stacked = unit
            .get::<Modifiers>()
            .into_iter()
            .flat_map(Modifiers::stacked);
        let granted = world.resource::<ModifierBook>().granted(stacked, status);
        let tags = world.resource::<TagBook>().unit_tags(unit_type, granted);
        world.insert_parts(id, tags);
    }

    fn set_blocks(world: &mut World, id: StableId, blocks: &[Block]) {
        let properties = blocks
            .iter()
            .fold(TagProperties::default(), |properties, &block| {
                properties.with_block(block)
            });
        Self::hold_properties(world, id, properties);
    }

    fn block_at(
        world: &mut World,
        id: StableId,
        tick: u64,
        stage: SimSystems,
        blocks: &'static [Block],
    ) {
        let block = move |world: &mut World| {
            if world.resource::<SimTick>().start() == Tick::new(tick) {
                Self::set_blocks(world, id, blocks);
            }
        };
        world.add_tick_systems(block.in_set(stage));
    }

    fn name_kinds(world: &mut World, damage_kinds: &[&str], pools: &[&str], resources: &[&str]) {
        let resources = Arc::new(resources.iter().copied().collect());
        let kinds = Arc::new(damage_kinds.iter().copied().collect());
        let pools: Names<PoolId> = pools.iter().copied().collect();
        let mut names = world.resource_mut::<MatchNames>();
        names.set_table::<DamageKind>(kinds);
        names.set_table::<ResourceId>(resources);
        names.set_table(Arc::new(pools));
    }
}

/// The test fixtures of `MatchNames` that need a crate above its own.
pub trait ViewFixtures {
    /// The place of `stat` among the stats the mode declares; `None` when it does not declare
    /// it.
    fn stat_index(&self, stat: &Stat) -> Option<StatId>;

    /// What a cost named `name` takes from: a pool, or else a player resource; `None` for a name
    /// the mode declares neither as.
    fn cost_target(&self, name: &str) -> Option<CostTarget>;
}

impl ViewFixtures for MatchNames {
    fn stat_index(&self, stat: &Stat) -> Option<StatId> {
        self.table::<StatId>()?.named(stat.name())
    }

    fn cost_target(&self, name: &str) -> Option<CostTarget> {
        let (no_pools, no_resources) = (Names::default(), Names::default());
        let pools = self.table::<PoolId>().unwrap_or(&no_pools);
        let resources = self.table().unwrap_or(&no_resources);
        CostTarget::named(pools, resources, name)
    }
}

/// The test fixtures of `Filter` that need a crate above its own.
pub trait FilterFixtures {
    /// The filter of `relations` alone, of a match of the release's engine tags.
    fn of_release_relations(relations: RelationSet) -> Filter;
}

impl FilterFixtures for Filter {
    fn of_release_relations(relations: RelationSet) -> Filter {
        Self::of_relations(relations, &TagNames::new(&Release::engine_tags()))
    }
}
