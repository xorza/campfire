use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt::Write;

use bevy_ecs::component::{Component, ComponentId};
use bevy_ecs::world::World;
use campfire_capabilities::Release;
use campfire_declare::{EntityKind, KindVisitor};
use campfire_sim::{EntityIndex, StableId};

/// The parts each entity of a running match held as a tick first found it, by its stable id:
/// after that, its parts change only by the parts of the states its kind declares, as each part
/// comes with the spawn (design 02, Direction). Each tick, it notes each entity whose parts
/// changed otherwise.
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
}

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
    /// The watch of `world`'s match, its kinds' states as the release declares them.
    pub fn of(world: &World) -> ArchetypeWatch {
        let mut kinds = KindStates::default();
        Release::kinds(&mut kinds);
        let components = world.components();
        let states = kinds.0.into_iter().filter_map(|(kind, state)| {
            Some((components.get_id(kind)?, components.get_id(state)?))
        });
        ArchetypeWatch {
            states: states.collect(),
            ..ArchetypeWatch::default()
        }
    }

    /// Reads each entity of `world`'s match after its tick `tick`, and notes each whose parts
    /// changed since a tick first found it but by its kind's states.
    pub fn tick(&mut self, world: &World, tick: u64) {
        for (id, entity) in world.resource::<EntityIndex>().iter() {
            let unit = world.entity(entity);
            let held = unit.archetype().components();
            self.parts.clear();
            self.parts.extend(held.iter().copied().filter(|&part| {
                !self
                    .states
                    .iter()
                    .any(|&(kind, state)| state == part && held.contains(&kind))
            }));
            self.parts.sort_unstable();
            match self.seen.get(&id) {
                Some(before) if *before != self.parts => {
                    let name = |&part: &ComponentId| world.components().get_name(part).unwrap();
                    let added = self.parts.iter().filter(|part| !before.contains(part));
                    let gone = before.iter().filter(|part| !self.parts.contains(part));
                    let added: Vec<_> = added.map(name).collect();
                    let gone: Vec<_> = gone.map(name).collect();
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
                    self.seen.insert(id, self.parts.clone());
                }
            }
        }
    }

    /// Fails on each change it noted.
    pub fn assert_kept(&self) {
        assert!(
            self.breaches.is_empty(),
            "parts that came or went after a spawn, none a state of its kind:\n{}",
            self.breaches
        );
    }
}
