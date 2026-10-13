use std::fmt::Debug;

use bevy_ecs::component::{Component, Immutable, Mutable};

use crate::replication::Replication;
use crate::{Declared, SendingWays};

/// How a state type is sent: each way is a type, which gives a reader of `SendingWays` the
/// method of that way and the bounds it takes. A type whose kind no client receives is
/// `NotSent`, and every other type is sent, or its list does not build: that check runs as the
/// build makes the code of a list, so a build or a test run reports it, and `cargo check` and
/// clippy, which make none, do not.
pub trait Sending<C: ?Sized> {
    /// Fails the build of a list whose type's kind and way of sending disagree, as the build
    /// evaluates it for the type.
    const CHECK: ();

    fn visit<T: SendingWays>(ways: &mut T);
}

/// Sent once, as its entity spawns: the type is immutable, so no code writes it after.
#[derive(Debug)]
pub struct SentOnce;

/// Sent as its entity spawns and on each insert after: the type is immutable, so an insert is
/// its one change, which a client must see, as a new owner (design 14, D17).
#[derive(Debug)]
pub struct SentOnInsert;

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
    const CHECK: () = assert!(
        C::KIND.is_sent(),
        "a type sent once is of a kind a client receives"
    );

    fn visit<T: SendingWays>(ways: &mut T) {
        let () = <Self as Sending<C>>::CHECK;
        ways.once::<C>(Declared(()));
    }
}

impl<C: Replication + Component<Mutability = Immutable>> Sending<C> for SentOnInsert {
    const CHECK: () = assert!(
        C::KIND.is_sent(),
        "a type sent on insert is of a kind a client receives"
    );

    fn visit<T: SendingWays>(ways: &mut T) {
        let () = <Self as Sending<C>>::CHECK;
        ways.on_insert::<C>(Declared(()));
    }
}

impl<C: Replication + Component<Mutability = Mutable>> Sending<C> for SentOnChange {
    const CHECK: () = assert!(
        C::KIND.is_sent(),
        "a type sent is of a kind a client receives"
    );

    fn visit<T: SendingWays>(ways: &mut T) {
        let () = <Self as Sending<C>>::CHECK;
        ways.on_change::<C>(Declared(()));
    }
}

impl<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug> Sending<C>
    for SentPredicted
{
    const CHECK: () = assert!(
        C::KIND.is_sent(),
        "a predicted type is of a kind a client receives"
    );

    fn visit<T: SendingWays>(ways: &mut T) {
        let () = <Self as Sending<C>>::CHECK;
        ways.predicted::<C>(Declared(()));
    }
}

impl<C: Replication> Sending<C> for NotSent {
    const CHECK: () = assert!(
        !C::KIND.is_sent(),
        "a type not sent is of the server's kind"
    );

    fn visit<T: SendingWays>(ways: &mut T) {
        let () = <Self as Sending<C>>::CHECK;
        ways.server::<C>(Declared(()));
    }
}
