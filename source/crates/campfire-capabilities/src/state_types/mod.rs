//! The state types each capability adds, named once in its `state_types` list, which every
//! consumer reads: the state registry, for the hash, the snapshot and the copy, and the network
//! layer, for what replicates, how and to whom (design 14, D9).

use std::fmt::Debug;

use bevy_ecs::component::{Component, Mutable};
use campfire_sim::{SimResource, StateRegistry};

use crate::state_types::kinded::Kinded;

pub(crate) mod data_kind;
pub(crate) mod kinded;

/// What reads the capabilities' lists of state types. Each method takes one type, by how it is
/// sent: the safe choice, on each change and not predicted, unless the list says otherwise.
pub trait StateTypes {
    /// `C`, sent on each change.
    fn component<C: Kinded + Component<Mutability = Mutable>>(&mut self);

    /// `C`, sent once, as it never changes after its entity spawns.
    fn component_once<C: Kinded + Component<Mutability = Mutable>>(&mut self);

    /// `C`, sent on each change, which its owner's client predicts.
    fn predicted<C: Kinded + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
    );

    /// `C`, one of `sim`'s own types, which the state registry holds from its start, predicted
    /// as `predicted` says.
    fn sim_predicted<C: Kinded + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
    );

    /// `R`, a resource, which no client receives (design 14, D6).
    fn resource<R: SimResource>(&mut self);
}

impl StateTypes for StateRegistry {
    fn component<C: Kinded + Component<Mutability = Mutable>>(&mut self) {
        self.register_component::<C>();
    }

    fn component_once<C: Kinded + Component<Mutability = Mutable>>(&mut self) {
        self.register_component::<C>();
    }

    fn predicted<C: Kinded + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
    ) {
        self.register_component::<C>();
    }

    fn sim_predicted<C: Kinded + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
    ) {
        assert!(
            self.holds::<C>(),
            "{} is a type the state registry holds from its start",
            C::NAME
        );
    }

    fn resource<R: SimResource>(&mut self) {
        self.register_resource::<R>();
    }
}
