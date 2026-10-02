use std::num::NonZeroU32;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::{Mut, World};
use campfire_math::{Num, SegmentSeed, Tick};
use campfire_script::rhai::Dynamic;
use campfire_script::{Budget, ScriptHost};
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, Position, SimTick, SimUpdate, StableId, StateRegistry,
    TickRate,
};

use crate::capability_set::CapabilitySet;
use crate::combat::Combat;
use crate::combat::internals;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::units::block::Block;
use crate::units::unit_tags::UnitTags;

/// A match for a capability's tests: a world that `SimUpdate::prepare` set up, with the core and
/// the declared capabilities installed and their schedule in it, and its state registry. A
/// module's harness holds one and adds only its own verbs.
#[derive(Debug)]
pub(crate) struct TestMatch {
    pub(crate) world: World,
    pub(crate) registry: StateRegistry,
}

impl TestMatch {
    /// The MOBA's 30 ticks a second.
    pub(crate) const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

    /// A match at `rate` of the capabilities `declared`, running scripts within `budgets`, or
    /// none, as a client; with combat, its life pool the first.
    pub(crate) fn new(
        declared: &[Capability],
        rate: TickRate,
        budgets: Option<ScriptBudgets>,
    ) -> TestMatch {
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), rate);
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        let set = CapabilitySet::new(declared).expect("a test declares a valid set");
        set.install(&mut world, &mut schedule, &mut registry, budgets);
        if set.contains(Capability::Combat) {
            internals::bind_life(&mut world, PoolId::FIRST);
        }
        world.add_schedule(schedule);
        TestMatch { world, registry }
    }

    /// Runs `install` on the world, its schedule and its registry, as a capability installs.
    pub(crate) fn install<R>(
        &mut self,
        install: impl FnOnce(&mut World, &mut Schedule, &mut StateRegistry) -> R,
    ) -> R {
        let registry = &mut self.registry;
        self.world.schedule_scope(SimUpdate, |world, schedule| {
            install(world, schedule, registry)
        })
    }

    /// A unit at `at` with `parts`, of the next stable id, with its type's tags when it has a
    /// type.
    pub(crate) fn spawn(&mut self, at: Position, parts: impl Bundle) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let entity = self.world.spawn((id, at, parts)).id();
        UnitTags::give_type_tags(&mut self.world, entity);
        id
    }

    pub(crate) fn entity(&self, id: StableId) -> Entity {
        self.world
            .resource::<EntityIndex>()
            .get(id)
            .expect("a unit of the match")
    }

    pub(crate) fn get<C: Component>(&self, id: StableId) -> &C {
        self.try_get(id).expect("a unit with the part")
    }

    /// Unit `id`'s part `C`; `None` when the unit is gone or lacks it.
    pub(crate) fn try_get<C: Component>(&self, id: StableId) -> Option<&C> {
        let entity = self.world.resource::<EntityIndex>().get(id)?;
        self.world.get::<C>(entity)
    }

    pub(crate) fn get_mut<C: Component<Mutability = Mutable>>(
        &mut self,
        id: StableId,
    ) -> Mut<'_, C> {
        let entity = self.entity(id);
        self.world
            .get_mut::<C>(entity)
            .expect("a unit with the part")
    }

    pub(crate) fn insert(&mut self, id: StableId, parts: impl Bundle) {
        let entity = self.entity(id);
        self.world.entity_mut(entity).insert(parts);
    }

    /// Gives unit `id` tags of no name that block `blocks`, in place of its own.
    pub(crate) fn set_blocks(&mut self, id: StableId, blocks: &[Block]) {
        self.insert(id, UnitTags::blocking(blocks));
    }

    /// The life unit `id` has left, exactly.
    pub(crate) fn life(&self, id: StableId) -> Num {
        let life = Combat::life(&self.world).expect("a match with a life pool");
        let pools = self.get::<Pools>(id);
        pools.current(life).expect("a unit with the life pool")
    }

    /// The life unit `id` has left, a whole amount.
    pub(crate) fn health(&self, id: StableId) -> i64 {
        self.life(id).to_int().expect("a whole amount of life")
    }

    /// The tick the match runs next.
    pub(crate) fn now(&self) -> Tick {
        self.world.resource::<SimTick>().start()
    }

    pub(crate) fn step(&mut self) {
        self.world.run_schedule(SimUpdate);
    }

    /// `probe(ctx, of)` in `source`, run on the units as they are now.
    pub(crate) fn probe(&mut self, source: &str, of: StableId) -> Result<Dynamic, CallError> {
        let ctx = self.world.non_send::<Ctx>().clone();
        ctx.view().read(&self.world);
        let unit = ctx.view().unit(of).expect("a unit of the match");
        let mut host = self.world.non_send_mut::<ScriptHost>();
        let script = host.compile(source).expect("a probe compiles");
        let mut budget = Budget::new(u64::MAX);
        host.call(&mut budget, script, "probe", (ctx, unit))
            .map_err(CallError::from_script)
    }

    /// `expression` evaluated with `ctx` and `of`, the unit `of`, on the units as they are now.
    pub(crate) fn read(&mut self, expression: &str, of: StableId) -> Result<Dynamic, CallError> {
        self.probe(&format!("fn probe(ctx, of) {{ {expression} }}"), of)
    }

    /// Runs ticks until the next is `tick`.
    pub(crate) fn run_until(&mut self, tick: u64) {
        while self.now().get() < tick {
            self.step();
        }
    }
}
