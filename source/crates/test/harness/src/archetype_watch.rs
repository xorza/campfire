use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::mem;
use std::sync::{Arc, Mutex};

use bevy_ecs::archetype::{Archetype, ArchetypeId};
use bevy_ecs::component::{Component, ComponentId};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::AddEvent;
use bevy_ecs::observer::{Observer, On};
use bevy_ecs::world::World;
use campfire_capabilities::Release;
use campfire_declare::{EntityKind, KindVisitor};
use campfire_sim::{EntityIndex, StableId};

/// The parts each entity of a running match held as a tick first found it, by its stable id:
/// after that, its parts change only by the parts of the states its kind declares, as each part
/// comes with the spawn as AGENTS.md's Direction asks. Each tick, it notes each entity whose parts
/// changed otherwise, and each entity new to it whose spawn moved it between archetypes more than
/// once, a move of its kind's states alone apart, as each insert that adds parts is a move.
#[derive(Debug, Default)]
pub struct ArchetypeWatch {
    /// Each kind's state parts, by the kind's component.
    states: Vec<(ComponentId, ComponentId)>,
    /// Each entity's parts but its states', in order.
    seen: BTreeMap<StableId, Vec<ComponentId>>,
    /// Each change it noted.
    breaches: String,
    /// Scratch for an entity's parts.
    parts: Vec<ComponentId>,
    /// The parts each insert that added parts to an entity added, by entity, since its last
    /// tick, as its observer records them: shared with the observer, so the match's world holds
    /// nothing of the watch's.
    moves: Arc<Mutex<SpawnMoves>>,
}

/// The moves that added parts to each entity, by entity, in order: the archetype each left, none
/// for its spawn, and the parts it added.
type SpawnMoves = BTreeMap<Entity, Vec<(Option<ArchetypeId>, Vec<ComponentId>)>>;

/// The kinds' state parts, by their types, as the declarations give them.
#[derive(Debug, Default)]
struct KindStates(Vec<(TypeId, TypeId)>);

impl KindVisitor for KindStates {
    fn kind<K: EntityKind>(&mut self) {}

    fn state<K: EntityKind, S: Component>(&mut self) {
        self.0.push((TypeId::of::<K>(), TypeId::of::<S>()));
    }
}

impl ArchetypeWatch {
    /// The watch of `world`'s match, its kinds' states as the release declares them, which
    /// observes each insert that adds parts from now on.
    pub fn of(world: &mut World) -> Self {
        let mut kinds = KindStates::default();
        Release::kinds(&mut kinds);
        let components = world.components();
        let states = kinds.0.into_iter().filter_map(|(kind, state)| {
            Some((components.get_id(kind)?, components.get_id(state)?))
        });
        let watch = Self {
            states: states.collect(),
            ..Self::default()
        };
        // Bevy triggers a lifecycle event only for parts with an observer of their own, and runs
        // an observer of many parts once for each of them a move adds: so it observes each part
        // the world registered, and counts a move once by the archetype it left.
        let moves = Arc::clone(&watch.moves);
        let observer = Observer::new(move |added: On<'_, '_, AddEvent>| {
            let trigger = added.trigger();
            let left = trigger.old_archetype.map(Archetype::id);
            let mut moves = moves.lock().unwrap();
            let entity = moves.entry(added.entity).or_default();
            if entity.last().is_none_or(|&(last, _)| last != left) {
                entity.push((left, trigger.components.to_vec()));
            }
        });
        let parts: Vec<ComponentId> = world
            .components()
            .iter_registered()
            .map(|(id, _)| id)
            .collect();
        world.spawn(observer.with_components(parts));
        watch
    }

    /// Reads each entity of `world`'s match after its tick `tick`, and notes each whose parts
    /// changed since a tick first found it but by its kind's states, and each new to it whose
    /// spawn took more than one move.
    pub fn tick(&mut self, world: &World, tick: u64) {
        let moves = mem::take(&mut *self.moves.lock().unwrap());
        let name = |part: ComponentId| world.components().get_name(part).unwrap();
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            let unit = world.entity(entity);
            let held = unit.archetype().components();
            let state = |part: ComponentId| {
                self.states
                    .iter()
                    .any(|&(kind, state)| state == part && held.contains(&kind))
            };
            self.parts.clear();
            self.parts
                .extend(held.iter().copied().filter(|&part| !state(part)));
            self.parts.sort_unstable();
            match self.seen.get(&id) {
                Some(before) if *before != self.parts => {
                    let added = self.parts.iter().filter(|part| !before.contains(part));
                    let gone = before.iter().filter(|part| !self.parts.contains(part));
                    let added: Vec<_> = added.map(|&part| name(part)).collect();
                    let gone: Vec<_> = gone.map(|&part| name(part)).collect();
                    writeln!(
                        self.breaches,
                        "tick {tick}, unit {}: + {added:?} - {gone:?}",
                        id.get()
                    )
                    .unwrap();
                    self.seen.insert(id, self.parts.clone());
                }
                Some(_) => {}
                None => {
                    let spawn = moves.get(&entity).map_or(&[][..], Vec::as_slice);
                    let spawn: Vec<Vec<_>> = spawn
                        .iter()
                        .filter(|(_, parts)| !parts.iter().all(|&part| state(part)))
                        .map(|(_, parts)| parts.iter().map(|&part| name(part)).collect())
                        .collect();
                    if spawn.len() > 1 {
                        writeln!(
                            self.breaches,
                            "tick {tick}, unit {} spawned in {} moves: {spawn:?}",
                            id.get(),
                            spawn.len()
                        )
                        .unwrap();
                    }
                    self.seen.insert(id, self.parts.clone());
                }
            }
        }
    }

    /// Fails on each change it noted.
    pub fn assert_kept(&self) {
        assert!(
            self.breaches.is_empty(),
            "parts that came or went after a spawn, none a state of its kind, or spawns of more \
             than one move:\n{}",
            self.breaches
        );
    }
}

#[cfg(test)]
mod tests {
    use std::any::type_name;

    use bevy_ecs::component::Component;
    use campfire_sim::IdAllocator;

    use super::*;

    #[derive(Component, Debug)]
    struct Early;

    #[derive(Component, Debug)]
    struct Late;

    #[test]
    fn a_spawn_of_two_inserts_is_noted_and_one_of_one_is_not() {
        let mut world = World::new();
        world.init_resource::<EntityIndex>();
        let mut ids = IdAllocator::default();
        // The watch observes the parts registered before it, as a match's install registers them.
        world.register_component::<StableId>();
        world.register_component::<Early>();
        world.register_component::<Late>();
        let mut watch = ArchetypeWatch::of(&mut world);
        let whole = ids.allocate();
        world.spawn((whole, Early, Late));
        let split = ids.allocate();
        let entity = world.spawn((split, Early)).id();
        world.entity_mut(entity).insert(Late);
        watch.tick(&world, 0);
        // The whole spawn, id 0, is one move; the split one, id 1, two: its spawn, then `Late`.
        assert_eq!((whole.get(), split.get()), (0, 1));
        let early = type_name::<Early>();
        let late = type_name::<Late>();
        let id = type_name::<StableId>();
        assert_eq!(
            watch.breaches,
            format!("tick 0, unit 1 spawned in 2 moves: [[{id:?}, {early:?}], [{late:?}]]\n")
        );
    }
}
