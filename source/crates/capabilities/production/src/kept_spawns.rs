use std::num::NonZeroUsize;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::system::Local;
use bevy_ecs::world::{DeferredWorld, World};
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_sim::{
    DueIndex, DueUnit, EntityIndex, SimComponent, SimTick, StableId, TimedKept, TimedProcess,
    TimedRuns,
};
use campfire_states::{NotSent, Replication};
use campfire_units::{Dead, Deaths, Spawner, UnitEntity, UnitsWorld};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::spawn_spec::{SpawnSpec, SpawnTurn};
use crate::trains::Trains;

/// A unit's kept spawns (04-capabilities/production.md, A building may keep spawns): the spawns
/// that live, by stable id, the tick each that died comes back at, earliest first, and the place
/// among its spec's unit types of the next spawn's, as it takes them in turn
/// (spawn-leaders.md, Spawns take their types in turn). Its hook pushes its unit into the index of
/// the spawns due at each insert, a restore's too, and each change that leaves a return pushes it
/// at its first. State of the server's.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[component(on_insert = KeptSpawns::pushed)]
pub(crate) struct KeptSpawns {
    living: Vec<StableId>,
    returns: Vec<Tick>,
    turn: u8,
}

/// A kept spawn's spawner, by stable id: the spawn comes back when it dies, and dies with its
/// spawner. State of the server's.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub(crate) struct SpawnOf {
    spawner: StableId,
}

impl KeptSpawns {
    /// Sends the spawns of the unit of `entity` out of its exit, each as a train leaves it, of its
    /// spec's types in turn, and keeps them: the server's, as it stands whole.
    pub(crate) fn send(world: &mut World, entity: Entity) {
        let count = Self::spec(world, entity)
            .expect("a unit that keeps spawns")
            .count;
        let spawner = *world.get::<StableId>(entity).expect("a spawner has an id");
        let mut kept = Self {
            living: Vec::with_capacity(NonZeroUsize::from(count).get()),
            returns: Vec::new(),
            turn: 0,
        };
        for _ in 0..count.get() {
            let turn = Self::next(world, entity, kept.turn);
            kept.living
                .push(Self::leave(world, entity, spawner, turn.unit_type));
            kept.turn = turn.next;
        }
        world.entity_mut(entity).insert(kept);
    }

    /// Follows the tick's deaths, in the order they came: a kept spawn's comes back its spec's
    /// time later, while its spawner lives, and a spawner's takes the life of each of its
    /// spawns that lives, which dies as the next tick's deaths find it. Each death costs a look
    /// of its own, and no unit that lives is visited.
    pub(crate) fn follow_deaths(world: &mut World, mut fallen: Local<'_, Vec<StableId>>) {
        let Some(deaths) = world.get_resource::<Deaths>() else {
            return;
        };
        fallen.clear();
        fallen.extend(deaths.iter().map(|death| death.fallen.id));
        let now = world.resource::<SimTick>().start();
        for &id in &*fallen {
            let Some(entity) = world.resource::<EntityIndex>().get(id) else {
                continue;
            };
            if let Some(&SpawnOf { spawner }) = world.get::<SpawnOf>(entity) {
                Self::lost(world, spawner, id, now);
            }
            let Some(mut kept) = world.get_mut::<Self>(entity) else {
                continue;
            };
            kept.returns.clear();
            let living = std::mem::take(&mut kept.living);
            for spawn in living {
                world.drain_life(spawn);
            }
        }
    }

    /// Sends each kept spawn whose return comes due this tick out of its spawner's exit, of the
    /// spawners the index holds due, in a buffer it keeps.
    pub(crate) fn bring_back(world: &mut World, mut due: Local<'_, Vec<DueUnit>>) {
        let now = world.resource::<SimTick>().start();
        due.clear();
        world
            .resource_mut::<DueIndex<Self>>()
            .take_due(now, &mut due);
        for &DueUnit { id, entity } in &*due {
            let Some(kept) = world.get::<Self>(entity) else {
                continue;
            };
            let back = kept.returns.iter().take_while(|&&at| at <= now).count();
            if back == 0 || world.entity(entity).contains::<Dead>() {
                continue;
            }
            for _ in 0..back {
                let turn = world.get::<Self>(entity).expect("its spawns").turn;
                let turn = Self::next(world, entity, turn);
                let spawn = Self::leave(world, entity, id, turn.unit_type);
                let mut kept = world.get_mut::<Self>(entity).expect("its spawns");
                let _ = kept.returns.remove(0);
                kept.living.push(spawn);
                kept.turn = turn.next;
            }
            Self::push_next(world, entity);
        }
    }

    /// The spawns the unit of `entity` keeps, of its type's spec.
    fn spec(world: &World, entity: Entity) -> Option<&SpawnSpec> {
        let unit_type = world.get::<UnitType>(entity)?;
        let book = world.get_resource::<ById<UnitType, SpawnSpec>>()?;
        book.get(*unit_type)
    }

    /// The unit type of the next spawn of the unit of `entity`, of its turn, and the turn after.
    fn next(world: &World, entity: Entity, turn: u8) -> SpawnTurn {
        Self::spec(world, entity)
            .expect("a unit that keeps spawns")
            .turn(turn)
    }

    /// A spawn of `unit_type` out of the exit of the spawner of `entity`, `spawner`, whose spawn
    /// it is; its id.
    fn leave(
        world: &mut World,
        entity: Entity,
        spawner: StableId,
        unit_type: UnitType,
    ) -> StableId {
        let exit = Trains::exit_spawn(world, entity, unit_type);
        let unit = Spawner::spawn(world, exit.at, SpawnOf { spawner });
        Trains::send_on(world, entity, unit, exit);
        exit.at.id
    }

    /// The spawn `id` of `spawner` died at `now`: it comes back its spec's time later, while its
    /// spawner lives.
    fn lost(world: &mut World, spawner: StableId, id: StableId, now: Tick) {
        let Some(entity) = world.resource::<EntityIndex>().get(spawner) else {
            return;
        };
        if world.entity(entity).contains::<Dead>() {
            return;
        }
        let Some(replace) = Self::spec(world, entity).map(|spec| spec.replace) else {
            return;
        };
        let Some(mut kept) = world.get_mut::<Self>(entity) else {
            return;
        };
        let Some(at) = kept.living.iter().position(|&spawn| spawn == id) else {
            return;
        };
        kept.living.remove(at);
        kept.returns.push(now.after(replace));
        Self::push_next(world, entity);
    }

    /// Pushes the spawner of `entity` into the index of the spawns due, at its first return.
    fn push_next(world: &mut World, entity: Entity) {
        let id = *world.get::<StableId>(entity).expect("a spawner has an id");
        let first = world
            .get::<Self>(entity)
            .and_then(|kept| kept.returns.first().copied());
        if let Some(first) = first {
            world
                .resource_mut::<DueIndex<Self>>()
                .push(first, id, entity);
        }
    }

    /// Pushes the spawner of the spawns into the index of the spawns due, at their first return,
    /// in a match that keeps spawns.
    fn pushed(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let (Some(kept), Some(&id)) = (world.get::<Self>(entity), world.get::<StableId>(entity))
        else {
            return;
        };
        let Some(&first) = kept.returns.first() else {
            return;
        };
        if let Some(mut index) = world.get_resource_mut::<DueIndex<Self>>() {
            index.push(first, id, entity);
        }
    }
}

/// Kept spawns come due at their first return.
impl TimedProcess for KeptSpawns {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = &'static Self;
    type Books = ();

    fn due((): &(), kept: &Self) -> Option<Tick> {
        kept.returns.first().copied()
    }
}

impl SimComponent for KeptSpawns {
    const NAME: &'static str = "production.kept_spawns";
    type Host = UnitEntity;

    // Its unit's spec keeps spawns, of which it holds at most the count, its returns from the
    // earliest, and its turn one of the spec's types.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let spec = Self::spec(world, entity);
        spec.is_some_and(|spec| {
            self.living.len() + self.returns.len() <= usize::from(spec.count.get())
                && usize::from(self.turn) < spec.unit_types.len()
        }) && self.returns.is_sorted()
    }
}

impl Replication for KeptSpawns {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

impl SimComponent for SpawnOf {
    const NAME: &'static str = "production.spawn_of";
    type Host = UnitEntity;

    // Its spawner may be gone, as a spawn outlives it until the next tick's deaths.
    fn check(&self, _: &World, _: Entity) -> bool {
        true
    }
}

impl Replication for SpawnOf {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
