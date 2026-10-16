//! The state types each module adds, named once in its declarations, which every consumer reads:
//! the state registry, for the hash, the snapshot and the copy, and the network layer, for what
//! replicates, how and to whom (14-replication-audiences.md, Each capability names its state types
//! once). Each type declares its kind and how it is sent; the list names it and nothing more.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};
use campfire_sim::{SimResource, StateRegistry};

mod data_kind;
mod recipients;
mod replication;

pub use crate::data_kind::DataKind;
pub use crate::recipients::Recipients;
pub use crate::replication::Replication;
pub use crate::replication::sending::{
    NotSent, Sending, SentOnChange, SentOnInsert, SentOnce, SentPredicted,
};

/// Proof that a call comes from a type's own `Sending`: only this module makes one, so a reader
/// cannot be told a way to send that its type does not declare.
#[derive(Debug)]
pub struct Declared(());

/// What reads the modules' lists of state types. A list calls `component`, `sim_component`,
/// `derived`, `derived_sent` and `resource`; a reader that needs how a component is sent asks its
/// type's `Sending`, through `SendingWays`.
pub trait StateTypes: Sized {
    /// `C`, a state type of a module, as its kind and its way of sending declare it.
    fn component<C: Replication>(&mut self);

    /// `C`, one of `sim`'s own types, which the state registry holds from its start.
    fn sim_component<C: Replication>(&mut self);

    /// `C`, a component no state holds, as each match derives it again from its state: its
    /// value is no part of the hash, the snapshot or what a client receives.
    fn derived<C: Component>(&mut self);

    /// `C`, derived as a derived component is, and no part of the hash or the snapshot, but sent on
    /// each change to its kind's audience, which cannot derive it: a player's supply, from units
    /// its audience may not hold (14-replication-audiences.md, Each capability names its state
    /// types once).
    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self);

    /// `R`, a resource, which no client receives (14-replication-audiences.md, A resource never
    /// replicates).
    fn resource<R: SimResource + Default>(&mut self);
}

/// The ways a state type is sent, each with the bounds its way takes, for a reader that registers
/// each type by its way: a type's `Sending` calls the one it declares.
pub trait SendingWays {
    /// `C`, sent once, as it never changes after its entity spawns.
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, declared: Declared);

    /// `C`, sent as its entity spawns and on each insert after, as an insert is its one change.
    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, declared: Declared);

    /// `C`, sent on each change.
    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, declared: Declared);

    /// `C`, sent on each change, which its owner's client predicts.
    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        declared: Declared,
    );

    /// `C`, which no client receives.
    fn server<C: Replication>(&mut self, declared: Declared);
}

impl StateTypes for StateRegistry {
    fn component<C: Replication>(&mut self) {
        let () = <C::Sending as Sending<C>>::CHECK;
        self.register_component::<C>();
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {
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
