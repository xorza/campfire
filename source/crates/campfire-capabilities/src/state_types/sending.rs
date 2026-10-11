use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};

use crate::state_types::replication::Replication;
use crate::state_types::{Declared, StateTypes};

/// How a state type is sent: each way is a type, which gives the reader of the lists the
/// method of that way and the bounds it takes. A type whose kind no client receives is
/// `NotSent`, and every other type is sent, or its list does not build: that check runs as the
/// build makes the code of a list, so a build or a test run reports it, and `cargo check` and
/// clippy, which make none, do not.
pub trait Sending<C: ?Sized> {
    fn visit<T: StateTypes>(types: &mut T);
}

/// Sent once, as its entity spawns: the type is immutable, so no code writes it after.
#[derive(Debug)]
pub struct SentOnce;

/// Sent on each change, not predicted.
#[derive(Debug)]
pub struct SentOnChange;

/// Sent on each change, and predicted by its owner's client, which compares it and rolls back.
#[derive(Debug)]
pub struct SentPredicted;

/// Kept on the server: its kind is `DataKind::Server`.
#[derive(Debug)]
pub struct NotSent;

impl<C: Replication + Component<Mutability = Immutable>> Sending<C> for SentOnce {
    fn visit<T: StateTypes>(types: &mut T) {
        const {
            assert!(
                C::KIND.is_sent(),
                "a type sent once is of a kind a client receives"
            );
        }
        types.once::<C>(Declared(()));
    }
}

impl<C: Replication + Component<Mutability = Mutable>> Sending<C> for SentOnChange {
    fn visit<T: StateTypes>(types: &mut T) {
        const {
            assert!(
                C::KIND.is_sent(),
                "a type sent is of a kind a client receives"
            );
        }
        types.on_change::<C>(Declared(()));
    }
}

impl<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug> Sending<C>
    for SentPredicted
{
    fn visit<T: StateTypes>(types: &mut T) {
        const {
            assert!(
                C::KIND.is_sent(),
                "a predicted type is of a kind a client receives"
            );
        }
        types.predicted::<C>(Declared(()));
    }
}

impl<C: Replication> Sending<C> for NotSent {
    fn visit<T: StateTypes>(types: &mut T) {
        const {
            assert!(
                !C::KIND.is_sent(),
                "a type not sent is of the server's kind"
            );
        }
        types.server::<C>(Declared(()));
    }
}
