//! The state types each capability adds, named once in its `state_types` list, which every
//! consumer reads: the state registry, for the hash, the snapshot and the copy, and the network
//! layer, for what replicates, how and to whom (design 14, D9). Each type declares its kind and
//! how it is sent; the list names it and nothing more.

use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use campfire_sim::{SimResource, StateRegistry};

use crate::state_types::replication::Replication;
use crate::state_types::sending::Sending;

pub(crate) mod data_kind;
pub(crate) mod replication;
pub(crate) mod sending;

/// Proof that a call comes from a type's own `Sending`: only this module makes one, so a list
/// cannot call a way to send that its type does not declare.
#[derive(Debug)]
pub struct Declared(());

/// What reads the capabilities' lists of state types. A list calls `component`, `sim_component`,
/// `derived` and `resource`; each component reaches the method of the way its type is sent.
pub trait StateTypes: Sized {
    /// `C`, sent once, as it never changes after its entity spawns.
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, declared: Declared);

    /// `C`, sent on each change.
    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, declared: Declared);

    /// `C`, sent on each change, which its owner's client predicts.
    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        declared: Declared,
    );

    /// `C`, which no client receives.
    fn server<C: Replication>(&mut self, declared: Declared);

    /// `C`, a component no state holds, as each match derives it again from its state: its
    /// value is no part of the hash, the snapshot or what a client receives.
    fn derived<C: Component>(&mut self);

    /// `R`, a resource, which no client receives (design 14, D6).
    fn resource<R: SimResource>(&mut self);

    /// `C`, as its type sends it.
    fn component<C: Replication>(&mut self) {
        <C::Sending as Sending<C>>::visit(self);
    }

    /// `C`, one of `sim`'s own types, which the state registry holds from its start, as its type
    /// sends it.
    fn sim_component<C: Replication>(&mut self) {
        self.component::<C>();
    }
}

impl StateTypes for StateRegistry {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.register_component::<C>();
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
        self.register_component::<C>();
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.register_component::<C>();
    }

    fn server<C: Replication>(&mut self, _: Declared) {
        self.register_component::<C>();
    }

    fn derived<C: Component>(&mut self) {}

    fn resource<R: SimResource>(&mut self) {
        self.register_resource::<R>();
    }

    fn sim_component<C: Replication>(&mut self) {
        assert!(
            self.holds::<C>(),
            "{} is a type the state registry holds from its start",
            C::NAME
        );
    }
}
