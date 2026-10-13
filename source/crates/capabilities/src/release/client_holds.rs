//! What a predicting client holds of each unit, by state type: the rule of design 14's D12, which
//! the prediction test and the sync test share.

use std::any::type_name;
use std::collections::BTreeMap;
use std::fmt::Debug;

use bevy_ecs::change_detection::{ComponentTicks, Tick};
use bevy_ecs::component::{Component, ComponentId, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::world::World;
use campfire_sim::{SimResource, StableId, Unpredicted};

use crate::declare::capability_set::CapabilitySet;
use crate::release::Release;
use crate::state_types::StateTypes;
use crate::state_types::data_kind::DataKind;
use crate::state_types::recipients::Recipients;
use crate::state_types::replication::Replication;

/// How a client comes to hold a state type: it derives it, or receives it as its kind's.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Held {
    Derived,
    Of(DataKind),
}

impl Held {
    /// Whether a client holds a type held so of a unit, which it predicts when `predicted`: one
    /// it derives always; of a unit it predicts, whose owner it is, every kind sent; of another,
    /// a kind whose default audience holds every client that holds the unit, or one of its own
    /// player's, which it reads through the owner of a unit it predicts (design 14, D12).
    pub(crate) const fn reaches(self, predicted: bool) -> bool {
        match self {
            Held::Derived => true,
            Held::Of(kind) if predicted => kind.is_sent(),
            Held::Of(kind) => {
                matches!(kind.audience(), Recipients::ALL)
                    || matches!(
                        kind,
                        DataKind::Resources | DataKind::Supply | DataKind::PlayerModifiers
                    )
            }
        }
    }
}

/// Each state type of the lists of a match, by its id in that match's world, with its name and
/// how a client holds it.
#[derive(Debug, Default)]
pub(crate) struct ClientHolds(BTreeMap<ComponentId, (&'static str, Held)>);

impl ClientHolds {
    /// The state types of a match of `set`, by their ids in `world`, which registers each.
    pub(crate) fn of(world: &mut World, set: CapabilitySet) -> ClientHolds {
        let mut classes = Classes {
            world,
            holds: ClientHolds::default(),
        };
        Release::state_types(set, &mut classes);
        classes.holds
    }

    /// The name of the state type `id` and how a client holds it; none for a type of no list.
    pub(crate) fn get(&self, id: ComponentId) -> Option<(&'static str, Held)> {
        self.0.get(&id).copied()
    }

    /// Takes from each unit of `world` that its client does not predict each state type it does
    /// not hold of such a unit, as a client holds the unit.
    pub(crate) fn strip(&self, world: &mut World) {
        for entity in ClientHolds::unpredicted(world) {
            let unheld: Vec<ComponentId> = world
                .entity(entity)
                .archetype()
                .components()
                .iter()
                .copied()
                .filter(|&id| self.get(id).is_some_and(|(_, held)| !held.reaches(false)))
                .collect();
            let mut unit = world.entity_mut(entity);
            for id in unheld {
                unit.remove_by_id(id);
            }
        }
    }

    /// Each state type a client receives that a unit of `world` it does not predict holds, by the
    /// unit's stable id and the type's name.
    pub(crate) fn parts(&self, world: &mut World) -> Vec<(StableId, &'static str)> {
        self.received(world, |_| true)
    }

    /// Each state type a client receives that a unit of `world` it does not predict changed after
    /// `since`, by the unit's stable id and the type's name: a client writes none, and derives
    /// the rest.
    pub(crate) fn writes(&self, world: &mut World, since: Tick) -> Vec<(StableId, &'static str)> {
        let now = world.read_change_tick();
        self.received(world, |ticks| ticks.is_changed(since, now))
    }

    /// Each state type a client receives that a unit of `world` it does not predict holds with
    /// change ticks that `keep` takes, by the unit's stable id and the type's name.
    fn received(
        &self,
        world: &mut World,
        keep: impl Fn(ComponentTicks) -> bool,
    ) -> Vec<(StableId, &'static str)> {
        let mut received = Vec::new();
        for entity in ClientHolds::unpredicted(world) {
            let unit = world.entity(entity);
            let id = *unit.get::<StableId>().expect("a unit has a stable id");
            for &component in unit.archetype().components() {
                let Some((name, Held::Of(_))) = self.get(component) else {
                    continue;
                };
                if unit.get_change_ticks_by_id(component).is_some_and(&keep) {
                    received.push((id, name));
                }
            }
        }
        received.sort_unstable();
        received
    }

    /// The units of `world` its client does not predict.
    pub(crate) fn unpredicted(world: &mut World) -> Vec<Entity> {
        let mut units = world.query_filtered::<Entity, With<Unpredicted>>();
        units.iter(world).collect()
    }
}

/// The walk of a match's lists that registers each state type and classes it.
#[derive(Debug)]
struct Classes<'w> {
    world: &'w mut World,
    holds: ClientHolds,
}

impl Classes<'_> {
    fn of<C: Component>(&mut self, held: Held) {
        let id = self.world.register_component::<C>();
        self.holds.0.insert(id, (type_name::<C>(), held));
    }
}

impl StateTypes for Classes<'_> {
    fn component<C: Replication>(&mut self) {
        self.of::<C>(Held::Of(C::KIND));
    }

    fn derived<C: Component>(&mut self) {
        self.of::<C>(Held::Derived);
    }

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.of::<C>(Held::Of(C::KIND));
    }

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {
        self.of::<C>(Held::Of(C::KIND));
    }
}
