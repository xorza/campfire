use std::num::NonZeroUsize;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_math::Vec3;
use campfire_sim::Position;
use campfire_sim::{
    DueIndex, DueUnit, EntityIndex, SimComponent, SimTick, StableId, TimedKept, TimedProcess,
    TimedRuns,
};
use campfire_states::{NotSent, Replication};
use campfire_units::{
    Dead, Deaths, Leader, LivingSpawns, SpawnOf, Spawner, UnitEntity, UnitsWorld,
};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::spawn_spec::{SpawnSpec, SpawnTurn};
use crate::trains::Trains;

/// A unit's kept spawns (04-capabilities/production.md, A building may keep spawns): the tick
/// each that died comes back at, earliest first, and the place among its spec's unit types of the
/// next spawn's, as it takes them in turn (04-capabilities/production.md, Spawns take their types
/// in turn); those that live are its `LivingSpawns`. Its hook pushes its unit into the index of the
/// spawns due at each insert, a restore's too, and each change that leaves a return pushes it at
/// its first. State of the server's.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[component(on_insert = DueIndex::<Self>::pushed)]
pub(crate) struct KeptSpawns {
    returns: Vec<Tick>,
    turn: u8,
}

impl KeptSpawns {
    /// Sends the spawns of the unit of `entity`, of its spec's types in turn, each as `leave`
    /// sends one, and keeps them: the server's, as it stands whole.
    pub(crate) fn send(world: &mut World, entity: Entity) {
        let count = Self::spec(world, entity)
            .expect("a unit that keeps spawns")
            .count;
        let spawner = *world.get::<StableId>(entity).expect("a spawner has an id");
        let kept = Self {
            returns: Vec::new(),
            turn: 0,
        };
        let living = LivingSpawns::with_capacity(NonZeroUsize::from(count).get());
        world.entity_mut(entity).insert((kept, living));
        for _ in 0..count.get() {
            Self::bring_one(world, entity, spawner);
        }
    }

    /// Follows the tick's deaths, in the order they came: a kept spawn's comes back its spec's
    /// time later, while its spawner lives, and a leader's last kills it, by the spawn's killer;
    /// a spawner's death takes the life of each of its spawns that lives, which dies as the next
    /// tick's deaths find it. Each death costs a look of its own, and no unit that lives is
    /// visited.
    pub(crate) fn follow_deaths(world: &mut World, mut fallen: Local<'_, Vec<TickDeath>>) {
        let Some(deaths) = world.get_resource::<Deaths>() else {
            return;
        };
        fallen.clear();
        fallen.extend(deaths.iter().map(|death| TickDeath {
            id: death.fallen.id,
            killer: death.killer,
        }));
        let now = world.resource::<SimTick>().start();
        for &TickDeath { id, killer } in &*fallen {
            let Some(entity) = world.resource::<EntityIndex>().get(id) else {
                continue;
            };
            if let Some(&spawn_of) = world.get::<SpawnOf>(entity) {
                Self::lost(world, spawn_of.spawner(), id, now, killer);
            }
            Self::fall(world, entity);
        }
    }

    /// Sends each kept spawn whose return comes due this tick, as `leave` sends one, of the
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
                let _ = world
                    .get_mut::<Self>(entity)
                    .expect("its spawns")
                    .returns
                    .remove(0);
                Self::bring_one(world, entity, id);
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

    /// Sends the next spawn of the spawner of `entity`, `spawner`, of its turn, which it then
    /// takes past, as one of its living spawns: out of its exit, as a train leaves it, or of a
    /// leader, at the cell nearest the living spawn nearest its place, or nearest its place with
    /// none, and rallying nowhere (04-capabilities/production.md, A later spawn buds from the
    /// nearest spawn).
    fn bring_one(world: &mut World, entity: Entity, spawner: StableId) {
        let turn = world.get::<Self>(entity).expect("its spawns").turn;
        let next = Self::next(world, entity, turn);
        let exit = if world.entity(entity).contains::<Leader>() {
            let place = world
                .get::<Position>(entity)
                .expect("a leader stands")
                .get();
            let near = Self::nearest(world, entity, place).unwrap_or(place);
            Trains::bud_spawn(world, entity, next.unit_type, near)
        } else {
            Trains::exit_spawn(world, entity, next.unit_type)
        };
        let unit = Spawner::spawn(world, exit.at, SpawnOf::new(spawner));
        Trains::send_on(world, entity, unit, exit);
        world.get_mut::<Self>(entity).expect("its spawns").turn = next.next;
        let mut living = world
            .get_mut::<LivingSpawns>(entity)
            .expect("its living spawns");
        living.push(exit.at.id);
    }

    /// The place of the living spawn of the spawner of `entity` nearest `place`, of a tie the
    /// first; none for no spawn in the match.
    fn nearest(world: &World, entity: Entity, place: Vec3) -> Option<Vec3> {
        let index = world.resource::<EntityIndex>();
        let living = world.get::<LivingSpawns>(entity)?;
        let places = living.iter().filter_map(|&spawn| {
            let at = world.get::<Position>(index.get(spawn)?)?.get();
            Some(at)
        });
        places.min_by_key(|&at| (at - place).length_squared_bits())
    }

    /// The spawn `id` of `spawner` died at `now`, by `killer`: it comes back its spec's time
    /// later, while its spawner lives, and a leader that loses its last dies by its killer
    /// (04-capabilities/production.md, A unit may lead its spawns).
    fn lost(
        world: &mut World,
        spawner: StableId,
        id: StableId,
        now: Tick,
        killer: Option<StableId>,
    ) {
        let Some(entity) = world.resource::<EntityIndex>().get(spawner) else {
            return;
        };
        if world.entity(entity).contains::<Dead>() {
            return;
        }
        let Some(replace) = Self::spec(world, entity).map(|spec| spec.replace) else {
            return;
        };
        let Some(mut living) = world.get_mut::<LivingSpawns>(entity) else {
            return;
        };
        if !living.remove(id) {
            return;
        }
        let last = living.is_empty();
        if last && world.entity(entity).contains::<Leader>() {
            world
                .get_mut::<Self>(entity)
                .expect("its spawns")
                .returns
                .clear();
            world.kill(spawner, killer);
            return;
        }
        world
            .get_mut::<Self>(entity)
            .expect("its spawns")
            .returns
            .push(now.after(replace));
        Self::push_next(world, entity);
    }

    /// The spawner of `entity` fell: none of its spawns comes back, and each that lives loses its
    /// life.
    fn fall(world: &mut World, entity: Entity) {
        let Some(mut kept) = world.get_mut::<Self>(entity) else {
            return;
        };
        kept.returns.clear();
        let mut living = world
            .get_mut::<LivingSpawns>(entity)
            .expect("its living spawns");
        let spawns = std::mem::take(&mut *living);
        for &spawn in &spawns {
            world.drain_life(spawn);
        }
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
}

/// A death of the tick, as `follow_deaths` follows it: the unit, and its killer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TickDeath {
    id: StableId,
    killer: Option<StableId>,
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

    // Its unit's spec keeps spawns, of which it holds at most the count, those that live and
    // those that come back, its returns from the earliest, its turn one of the spec's types, and
    // its unit a leader exactly when the spec leads them.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let spec = Self::spec(world, entity);
        let living = world
            .get::<LivingSpawns>(entity)
            .map_or(0, LivingSpawns::len);
        let leads = world.entity(entity).contains::<Leader>();
        spec.is_some_and(|spec| {
            living + self.returns.len() <= usize::from(spec.count.get())
                && usize::from(self.turn) < spec.unit_types.len()
                && spec.led == leads
        }) && self.returns.is_sorted()
    }
}

impl Replication for KeptSpawns {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
