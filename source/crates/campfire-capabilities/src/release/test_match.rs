use std::collections::BTreeMap;
use std::num::NonZeroU32;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Query, Res};
use bevy_ecs::world::{Mut, World};
use campfire_codec::Binary;
use campfire_common::{SegmentSeed, Tick};
use campfire_log::internals::LogCheck;
use campfire_math::Num;
use campfire_script::rhai::Dynamic;
use campfire_script::{Budget, ScriptHost};
use campfire_sim::{
    Capability, EntityIndex, IdAllocator, Position, SimSet, SimTick, SimUpdate, SnapshotError,
    StableId, StateCopy, StateRegistry, TickRate,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::actions::slot_kinds::SlotKinds;
use crate::units::Units;
use crate::units::tag_book::TagBook;
use crate::units::tag_data::TagData;
use crate::units::team::Team;
use crate::units::unit_core::UnitCore;
use crate::units::unit_type::UnitType;
use crate::units::unit_type_data::UnitTypeData;
use crate::units::view::View;

use crate::actions::slot_kind_data::SlotKindData;
use crate::combat::internals;
use crate::declare::capability_set::CapabilitySet;
use crate::players::match_entity::MatchEntity;
use crate::release::Release;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::script_book::ScriptBook;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::stats::life::Life;
use crate::stats::pool_id::PoolId;
use crate::units::block::Block;
use crate::units::unit_tags::UnitTags;
use crate::values::declared_name::DeclaredName;

/// A match for a capability's tests: a world that `SimUpdate::prepare` set up, with the core and
/// the declared capabilities installed and their schedule in it, and its state registry. A
/// module's harness holds one and adds only its own verbs.
#[derive(Debug)]
pub(crate) struct TestMatch {
    pub(crate) world: World,
    pub(crate) registry: StateRegistry,
    /// The type of the units a test spawns with no type of its own, once the first spawn sealed
    /// the match's unit types.
    unit_type: Option<UnitType>,
    /// Last, so it drops after the world and sees what the world logs as it drops.
    _log: LogCheck,
}

impl TestMatch {
    /// The MOBA's 30 ticks a second.
    pub(crate) const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

    /// A match at `rate` of the capabilities `declared`, running scripts within `budgets`, or
    /// none, as a client; with combat, its life pool the first. It has no mode to start it, so it
    /// starts with the match's entity, as a start spawns it.
    pub(crate) fn new(
        declared: &[Capability],
        rate: TickRate,
        budgets: Option<ScriptBudgets>,
    ) -> TestMatch {
        let mut started = TestMatch::unstarted(declared, rate, budgets);
        MatchEntity::start_without_mode(&mut started.world);
        started
    }

    /// The match `new` gives, with no entity yet, for a mode to start.
    pub(crate) fn unstarted(
        declared: &[Capability],
        rate: TickRate,
        budgets: Option<ScriptBudgets>,
    ) -> TestMatch {
        let log = LogCheck::start();
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]), rate);
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        let set = CapabilitySet::new(declared).expect("a test declares a valid set");
        Release::install(set, &mut world, &mut schedule, &mut registry, budgets);
        // A match installs the book its package load read; a test fills one as it compiles.
        world.insert_resource(ScriptBook::default());
        // A mode declares the kinds its units' slots are of: here one, whose one rank comes with
        // the spawn, which a test that needs others replaces.
        if world.contains_resource::<SlotKinds>() {
            let basic = SlotKindData {
                name: DeclaredName::new("basic").unwrap(),
                ranks: None,
            };
            world.insert_resource(SlotKinds(vec![basic]));
        }
        if set.contains(Capability::Combat) {
            internals::bind_life(&mut world, PoolId::FIRST);
        }
        world.add_schedule(schedule);
        TestMatch {
            world,
            registry,
            unit_type: None,
            _log: log,
        }
    }

    /// A client's match at `RATE` of `declared`: no script host, and no system that runs scripts.
    pub(crate) fn client(declared: &[Capability]) -> TestMatch {
        TestMatch::new(declared, TestMatch::RATE, None)
    }

    /// Whether `value`, encoded as a snapshot holds it, decodes again: a decode keeps the rules a
    /// type's own values follow, as a restore check keeps those that need the match.
    pub(crate) fn decodes<T: Serialize + DeserializeOwned>(value: &T) -> bool {
        Binary::decode::<T>(&Binary::encode(value)).is_ok()
    }

    /// A server's match at `RATE` of `declared`, running scripts within `budgets`.
    pub(crate) fn server(declared: &[Capability], budgets: ScriptBudgets) -> TestMatch {
        TestMatch::new(declared, TestMatch::RATE, Some(budgets))
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

    /// A unit of `team` at `at` with `parts`, of the match's own type, `unit_type`, spawned as
    /// `spawn_of` spawns one.
    pub(crate) fn spawn(&mut self, at: Position, team: Team, parts: impl Bundle) -> StableId {
        let unit_type = self.unit_type();
        self.spawn_of(unit_type, at, team, parts)
    }

    /// A unit of `unit_type` and `team` at `at`, of the next stable id, spawned as a mode's unit
    /// and a delivery are: `parts`, then its core, with its type's tags and its script state from
    /// the match's books. The first spawn seals the match's unit types, as a match loads them all
    /// before any unit spawns.
    pub(crate) fn spawn_of(
        &mut self,
        unit_type: UnitType,
        at: Position,
        team: Team,
        parts: impl Bundle,
    ) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let core = UnitCore {
            id,
            pos: at,
            unit_type,
            team,
            owner: None,
        };
        self.spawn_core(core, parts)
    }

    /// The unit of `core`, with `parts`, spawned as `spawn_of` spawns one.
    pub(crate) fn spawn_core(&mut self, core: UnitCore, parts: impl Bundle) -> StableId {
        self.seal_types();
        core.spawn(&mut self.world, parts);
        core.id
    }

    /// The type of the units a test spawns with no type of its own: one of no tag, param or
    /// state field, which sealing the match's unit types loads.
    pub(crate) fn unit_type(&mut self) -> UnitType {
        self.seal_types();
        self.unit_type
            .expect("a test that puts its own tag book spawns units of its own types")
    }

    /// Seals the match's unit types, once, with no effect for any tag but an engine tag's own,
    /// as `seal_types_with` does.
    fn seal_types(&mut self) {
        if self.unit_type.is_some() || self.world.contains_resource::<TagBook>() {
            return;
        }
        self.seal_types_with(&BTreeMap::new());
    }

    /// Seals the match's unit types: loads the match's own type, and gives the tag book their
    /// tags and `effects`, by tag, as a match's books take them once its packages load. A test
    /// that put a tag book of its own sealed them itself, and its match has no type of its own.
    pub(crate) fn seal_types_with(&mut self, effects: &BTreeMap<DeclaredName, TagData>) {
        assert_eq!(self.unit_type, None, "a match seals its unit types once");
        let unit_type = Units::load_next_type(&mut self.world, &UnitTypeData::default());
        self.unit_type = Some(unit_type);
        let book = self.world.non_send::<View>().types_mut().tag_book(effects);
        self.world.insert_resource(book);
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

    /// Restores its state into `fresh`, a match that loaded the same books, as a restore does; the
    /// error that refuses it, if one does. A match whose spawns sealed its unit types seals
    /// `fresh`'s, which loads the same type of its own.
    pub(crate) fn refusal_into(&self, fresh: &mut TestMatch) -> Option<SnapshotError> {
        if self.unit_type.is_some() {
            fresh.seal_types();
            assert_eq!(
                fresh.unit_type, self.unit_type,
                "the fresh match loaded the same types"
            );
        }
        fresh.clear();
        let mut snapshot = Vec::new();
        self.registry.snapshot(&self.world, &mut snapshot);
        self.registry.restore(&snapshot, &mut fresh.world).err()
    }

    /// Gives unit `id` tags of no name that block `blocks`, in place of its own.
    pub(crate) fn set_blocks(&mut self, id: StableId, blocks: &[Block]) {
        self.insert(id, UnitTags::blocking(blocks));
    }

    /// The life unit `id` has left, exactly.
    pub(crate) fn life(&self, id: StableId) -> Num {
        self.get::<Life>(id).current()
    }

    /// The life unit `id` has left, a whole amount.
    pub(crate) fn health(&self, id: StableId) -> i64 {
        self.life(id).to_int().expect("a whole amount of life")
    }

    /// Despawns the match's entity it started with, as a restore takes a world of no entity.
    pub(crate) fn clear(&mut self) {
        if let Some(entity) = MatchEntity::entity(&self.world) {
            self.world.despawn(entity);
        }
    }

    /// Restores its state into `fresh`, a match that loaded the same books, as a restore does, and
    /// asserts that the two hash the same.
    pub(crate) fn restore_into(&self, fresh: &mut TestMatch) {
        assert_eq!(
            self.refusal_into(fresh),
            None,
            "a match's own snapshot restores"
        );
        assert_eq!(
            self.registry.hash(&fresh.world),
            self.registry.hash(&self.world),
            "the restored match hashes as the match"
        );
    }

    /// Gives unit `id` tags of no name that block `blocks` in `set` of tick `tick`, as a modifier
    /// applied in that stage would.
    pub(crate) fn block_at(
        &mut self,
        id: StableId,
        tick: u64,
        set: SimSet,
        blocks: &'static [Block],
    ) {
        let entity = self.entity(id);
        let block = move |now: Res<'_, SimTick>, mut tags: Query<'_, '_, &mut UnitTags>| {
            if now.start() == Tick::new(tick) {
                *tags.get_mut(entity).expect("a unit with tags") = UnitTags::blocking(blocks);
            }
        };
        self.install(|_, schedule, _| {
            schedule.add_systems(block.in_set(set));
        });
    }

    /// The tick the match runs next.
    pub(crate) fn now(&self) -> Tick {
        self.world.resource::<SimTick>().start()
    }

    /// Runs one tick; no `App` updates its world, so it clears the world's change trackers after
    /// the tick, as the runner's `Session::run_tick_alone` does.
    pub(crate) fn step(&mut self) {
        self.world.run_schedule(SimUpdate);
        self.world.clear_trackers();
    }

    /// Brings `copy`, a copy of the match's state, up to the match, and checks that every state
    /// type of it hashes as the match's own.
    pub(crate) fn check_copy(&mut self, copy: &mut StateCopy) {
        copy.follow(&self.registry, &mut self.world);
        let (mut copied, mut own) = (Vec::new(), Vec::new());
        self.registry.hash_by_type(copy.world(), &mut copied);
        self.registry.hash_by_type(&self.world, &mut own);
        let differ: Vec<&str> = copied
            .iter()
            .zip(&own)
            .filter(|(copied, own)| copied.hash != own.hash)
            .map(|(copied, _)| copied.name)
            .collect();
        assert!(
            differ.is_empty(),
            "the copy differs before tick {}: {differ:?}",
            self.now().get()
        );
    }

    /// `probe(ctx, of)` in `source`, run on the units as they are now.
    pub(crate) fn probe(&mut self, source: &str, of: StableId) -> Result<Dynamic, CallError> {
        let ctx = self.world.non_send::<Ctx>().clone();
        ctx.view().read(&mut self.world);
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
