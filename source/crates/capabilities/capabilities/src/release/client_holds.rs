//! What a predicting client holds of each unit, by state type: the rule of
//! (10-replication-audiences.md, Prediction reads only what its client holds), which
//! the prediction test and the sync test share.

use std::any::type_name;
use std::collections::BTreeMap;
use std::fmt::Debug;

use bevy_ecs::change_detection::{ComponentTicks, Tick};
use bevy_ecs::component::{Component, ComponentId, Immutable, Mutable};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, With};
use bevy_ecs::world::World;
use campfire_declarations::CapabilitySet;
use campfire_sim::{SimResource, StableId, Unpredicted};
use campfire_states::{Declared, Replication, Sending, SendingWays, StateTypes};
use campfire_visibility::{DataKind, Recipients};

use crate::release::Release;

/// How a client comes to hold a state type: it derives it, or receives it as its kind's, and
/// writes it on the units it predicts when `written`, as it predicts it, which a rollback
/// compares, or derives it as the server does.
#[derive(Debug, Clone, Copy)]
pub enum ClientHolding {
    Derived,
    Of { kind: DataKind, written: bool },
}

impl ClientHolding {
    /// Whether a client holds a type held so of a unit, which it predicts when `predicted`: one it
    /// derives always; of a unit it predicts, whose owner it is, every kind sent; of another, a
    /// kind whose default audience holds every client that holds the unit, or one of its own
    /// player's, which it reads through the owner of a unit it predicts
    /// (10-replication-audiences.md, Prediction reads only what its client holds).
    pub const fn reaches(self, predicted: bool) -> bool {
        match self {
            Self::Derived => true,
            Self::Of { kind, .. } if predicted => kind.is_sent(),
            Self::Of { kind, .. } => {
                matches!(kind.audience(), Recipients::ALL) || kind.is_players()
            }
        }
    }
}

/// Each state type of the lists of a match, by its id in that match's world, with its name and
/// how a client holds it.
#[derive(Debug, Default)]
pub struct ClientHolds(BTreeMap<ComponentId, (&'static str, ClientHolding)>);

impl ClientHolds {
    /// The state types of a match of `set`, by their ids in `world`, which registers each.
    pub fn of(world: &mut World, set: CapabilitySet) -> Self {
        let mut classes = Classes {
            world,
            holds: Self::default(),
        };
        Release::state_types(set, &mut classes);
        classes.holds
    }

    /// The name of the state type `id` and how a client holds it; none for a type of no list.
    pub fn get(&self, id: ComponentId) -> Option<(&'static str, ClientHolding)> {
        self.0.get(&id).copied()
    }

    /// Takes from each unit of `world` that its client does not predict each state type it does
    /// not hold of such a unit, as a client holds the unit.
    pub(crate) fn strip(&self, world: &mut World) {
        for entity in Self::unpredicted(world) {
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
        self.received(world, |held| !held.predicted)
    }

    /// Each state type a client receives that a unit of `world` changed after `since`, where the
    /// client writes none, by the unit's stable id and the type's name: on a unit it does not
    /// predict, every one; on a unit it predicts, one it neither predicts nor derives
    /// (10-replication-audiences.md, A client writes only what it predicts or derives).
    pub(crate) fn writes(&self, world: &mut World, since: Tick) -> Vec<(StableId, &'static str)> {
        let now = world.read_change_tick();
        self.received(world, |held| {
            !(held.predicted && held.written) && held.ticks.is_changed(since, now)
        })
    }

    /// Each state type a client receives that a unit of `world` holds, as `keep` takes it, by the
    /// unit's stable id and the type's name.
    fn received(
        &self,
        world: &mut World,
        keep: impl Fn(ReceivedPart) -> bool,
    ) -> Vec<(StableId, &'static str)> {
        let mut units = world.query_filtered::<(Entity, Has<Unpredicted>), With<StableId>>();
        let units: Vec<_> = units.iter(world).collect();
        let mut received = Vec::new();
        for (entity, unpredicted) in units {
            let unit = world.entity(entity);
            let id = *unit.get::<StableId>().unwrap();
            for &component in unit.archetype().components() {
                let Some((name, ClientHolding::Of { written, .. })) = self.get(component) else {
                    continue;
                };
                let ticks = unit.get_change_ticks_by_id(component).unwrap();
                let held = ReceivedPart {
                    predicted: !unpredicted,
                    written,
                    ticks,
                };
                if keep(held) {
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

/// A state type a unit holds, of those a client receives: whether the client predicts the unit,
/// whether it writes the type there, and the type's change ticks.
#[derive(Debug, Clone, Copy)]
struct ReceivedPart {
    predicted: bool,
    written: bool,
    ticks: ComponentTicks,
}

/// The walk of a match's lists that registers each state type and classes it.
#[derive(Debug)]
struct Classes<'w> {
    world: &'w mut World,
    holds: ClientHolds,
}

impl Classes<'_> {
    fn of<C: Component>(&mut self, held: ClientHolding) {
        let id = self.world.register_component::<C>();
        self.holds.0.insert(id, (type_name::<C>(), held));
    }

    /// A type a client receives, and writes on the units it predicts as their owner's client
    /// predicts it.
    fn sent<C: Replication>(&mut self) {
        let mut predicts = OwnerPredicts(false);
        <C::Sending as Sending<C>>::visit(&mut predicts);
        self.of::<C>(ClientHolding::Of {
            kind: C::KIND,
            written: predicts.0,
        });
    }
}

impl StateTypes for Classes<'_> {
    fn component<C: Replication>(&mut self) {
        self.sent::<C>();
    }

    fn derived<C: Component>(&mut self) {
        self.of::<C>(ClientHolding::Derived);
    }

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.of::<C>(ClientHolding::Of {
            kind: C::KIND,
            written: true,
        });
    }

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {
        self.sent::<C>();
    }
}

/// Whether a type's owner's client predicts it, as its way of sending says.
#[derive(Debug)]
struct OwnerPredicts(bool);

impl SendingWays for OwnerPredicts {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {}

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {}

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {}

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.0 = true;
    }

    fn server<C: Replication>(&mut self, _: Declared) {}
}
