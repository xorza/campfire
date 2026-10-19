//! Hierarchy members follow the replication aspects of their root.
//!
//! The policy is in (13-net-stack/replication.md, Hierarchy inheritance).
use crate::deferred_entity_commands::DeferredEntityCommands;
use crate::manual_visibility::ManualVisibility;
use crate::overridden::Overridden;
use crate::replicate_like::ReplicateLike;
use crate::replicate_like::ReplicateLikeChildren;
use crate::replication_target::InterpolationTarget;
use crate::replication_target::PredictionTarget;
use crate::replication_target::Replicate;
use crate::rooms::Rooms;
use crate::visibility_bits::VisibilityBits;
use bevy_app::prelude::*;
use bevy_ecs::component::Immutable;
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryData;
use bevy_ecs::system::SystemParam;
use bevy_replicon::server::visibility::client_visibility::ClientVisibility;
use core::fmt::Debug;

/// Keeps each aspect that a hierarchy member follows equal to its root's
/// (13-net-stack/replication.md, Detaching never widens what an entity replicates).
#[derive(Debug)]
pub(crate) struct InheritancePlugin;

/// What an entity keeps of an aspect it followed when it leaves its root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnDetach {
    /// The copy goes with the attachment: the aspect lets the entity replicate.
    Drop,
    /// The copy becomes the entity's own: the aspect limits where the entity replicates.
    Keep,
}

/// A component that hierarchy members follow from their root unless they hold
/// [`Overridden<Self>`].
trait Aspect: Component<Mutability = Immutable> + Clone + PartialEq + Debug {
    const ON_DETACH: OnDetach;
}

impl Aspect for Replicate {
    const ON_DETACH: OnDetach = OnDetach::Drop;
}

impl Aspect for PredictionTarget {
    const ON_DETACH: OnDetach = OnDetach::Drop;
}

impl Aspect for InterpolationTarget {
    const ON_DETACH: OnDetach = OnDetach::Drop;
}

impl Aspect for Rooms {
    const ON_DETACH: OnDetach = OnDetach::Keep;
}

#[derive(Debug, QueryData)]
struct MemberAspect<T: Aspect> {
    value: Option<&'static T>,
    overridden: Has<Overridden<T>>,
}

impl<T: Aspect> MemberAspectItem<'_, '_, T> {
    /// An entity that joins keeps a `T` that differs from its root's as an override, and
    /// follows the root otherwise.
    fn join(&self, member: Entity, root: Option<&T>, deferred: &mut DeferredEntityCommands) {
        if self.overridden {
            return;
        }
        match self.value {
            Some(own) if root != Some(own) => {
                deferred.insert(member, Overridden::<T>::default());
            }
            Some(_) => {}
            None => {
                if let Some(follow) = FollowWrite::of(None, root) {
                    follow.defer(member, deferred);
                }
            }
        }
    }

    fn follow(&self, root: Option<&T>) -> Option<FollowWrite<T>> {
        if self.overridden {
            None
        } else {
            FollowWrite::of(self.value, root)
        }
    }

    fn reattach(&self, member: Entity, root: Option<&T>, deferred: &mut DeferredEntityCommands) {
        if let Some(follow) = self.follow(root) {
            follow.defer(member, deferred);
        }
    }

    fn detach(&self, member: Entity, deferred: &mut DeferredEntityCommands) {
        if self.overridden {
            deferred.remove::<Overridden<T>>(member);
        } else if T::ON_DETACH == OnDetach::Drop && self.value.is_some() {
            deferred.remove::<T>(member);
        }
    }
}

/// The write that makes a member's copy equal to its root's value, absence included
/// (13-net-stack/replication.md, Absence is a value).
#[derive(Debug)]
enum FollowWrite<T> {
    Insert(T),
    Remove,
}

impl<T: Aspect> FollowWrite<T> {
    /// The write that turns `current` into `root`, or `None` when they are equal.
    fn of(current: Option<&T>, root: Option<&T>) -> Option<Self> {
        match (current, root) {
            (Some(current), Some(root)) if current == root => None,
            (_, Some(root)) => Some(Self::Insert(root.clone())),
            (Some(_), None) => Some(Self::Remove),
            (None, None) => None,
        }
    }

    fn defer(self, member: Entity, deferred: &mut DeferredEntityCommands) {
        match self {
            Self::Insert(value) => deferred.insert(member, value),
            Self::Remove => deferred.remove::<T>(member),
        }
    }

    fn queue(self, member: Entity, commands: &mut Commands<'_, '_>) {
        match self {
            Self::Insert(value) => {
                commands.entity(member).try_insert(value);
            }
            Self::Remove => {
                commands.entity(member).try_remove::<T>();
            }
        }
    }
}

#[derive(Debug, QueryData)]
struct Aspects {
    replicate: MemberAspect<Replicate>,
    prediction: MemberAspect<PredictionTarget>,
    interpolation: MemberAspect<InterpolationTarget>,
    rooms: MemberAspect<Rooms>,
    visibility_overridden: Has<Overridden<ManualVisibility>>,
}

/// What a change of attachment reads and writes: the aspects of the member and its root, and the
/// manual visibility of every sender.
#[derive(Debug, SystemParam)]
struct LinkAccess<'w, 's> {
    likes: Query<'w, 's, &'static ReplicateLike>,
    aspects: Query<'w, 's, Aspects>,
    senders: Query<'w, 's, &'static mut ClientVisibility>,
    bits: Option<Res<'w, VisibilityBits>>,
}

/// The kind of write that an observer of `T` saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Write {
    Insert,
    Remove,
}

/// What a write of `T` reads: the member's attachment, the root's members, and `T` on both.
#[derive(Debug, SystemParam)]
struct WriteAccess<'w, 's, T: Aspect> {
    likes: Query<'w, 's, &'static ReplicateLike>,
    members: Query<'w, 's, &'static ReplicateLikeChildren>,
    aspects: Query<'w, 's, MemberAspect<T>>,
}

impl<T: Aspect> WriteAccess<'_, '_, T> {
    /// A write on a member overrides its root's `T` when it leaves the member's `T` different from
    /// the root's, and follows the root when it leaves them equal. A write on a root reaches every
    /// member that follows it (13-net-stack/replication.md, A write equal to the root's value is a
    /// follow).
    fn written(&self, entity: Entity, write: Write, commands: &mut Commands<'_, '_>) {
        let value = match write {
            Write::Insert => self.aspects.get(entity).ok().and_then(|own| own.value),
            Write::Remove => None,
        };
        if let Ok(like) = self.likes.get(entity) {
            let root = self.aspects.get(like.root).ok().and_then(|root| root.value);
            let overridden = self.aspects.get(entity).is_ok_and(|own| own.overridden);
            match (value == root, overridden) {
                (false, false) => {
                    commands
                        .entity(entity)
                        .try_insert(Overridden::<T>::default());
                }
                (true, true) => {
                    commands.entity(entity).try_remove::<Overridden<T>>();
                }
                _ => {}
            }
            return;
        }
        let Ok(members) = self.members.get(entity) else {
            return;
        };
        for member in members.iter() {
            if let Some(follow) = self
                .aspects
                .get(member)
                .ok()
                .and_then(|own| own.follow(value))
            {
                follow.queue(member, commands);
            }
        }
    }
}

impl InheritancePlugin {
    fn register<T: Aspect>(app: &mut App) {
        app.add_observer(Self::inserted::<T>);
        app.add_observer(Self::removed::<T>);
        app.add_observer(Self::override_removed::<T>);
    }

    fn inserted<T: Aspect>(
        trigger: On<'_, '_, Insert<T>>,
        access: WriteAccess<'_, '_, T>,
        mut commands: Commands<'_, '_>,
    ) {
        access.written(trigger.entity, Write::Insert, &mut commands);
    }

    fn removed<T: Aspect>(
        trigger: On<'_, '_, Remove<T>>,
        access: WriteAccess<'_, '_, T>,
        mut commands: Commands<'_, '_>,
    ) {
        access.written(trigger.entity, Write::Remove, &mut commands);
    }

    /// A member that loses [`Overridden<T>`] follows its root's `T` again.
    fn override_removed<T: Aspect>(
        trigger: On<'_, '_, Remove<Overridden<T>>>,
        likes: Query<'_, '_, &ReplicateLike>,
        aspects: Query<'_, '_, MemberAspect<T>>,
        mut commands: Commands<'_, '_>,
    ) {
        let member = trigger.entity;
        let Ok(like) = likes.get(member) else {
            return;
        };
        let (Ok(own), Ok(root)) = (aspects.get(member), aspects.get(like.root)) else {
            return;
        };
        if let Some(follow) = FollowWrite::of(own.value, root.value) {
            follow.queue(member, &mut commands);
        }
    }

    /// A member that loses [`Overridden<ManualVisibility>`] follows its root's manual visibility
    /// again.
    fn visibility_override_removed(
        trigger: On<'_, '_, Remove<Overridden<ManualVisibility>>>,
        likes: Query<'_, '_, &ReplicateLike>,
        mut senders: Query<'_, '_, &mut ClientVisibility>,
        bits: Option<Res<'_, VisibilityBits>>,
    ) {
        let (Ok(like), Some(bits)) = (likes.get(trigger.entity), bits) else {
            return;
        };
        for mut visibility in &mut senders {
            bits.copy(&mut visibility, trigger.entity, like.root);
        }
    }

    /// An entity that joins a hierarchy keeps each aspect it holds that differs from its root's
    /// as an override, and follows every other aspect.
    fn join(
        trigger: On<'_, '_, Add<ReplicateLike>>,
        mut access: LinkAccess<'_, '_>,
        mut deferred: Deferred<'_, DeferredEntityCommands>,
    ) {
        let member = trigger.entity;
        let Ok(&ReplicateLike { root }) = access.likes.get(member) else {
            return;
        };
        let (Ok(own), Ok(root_aspects)) = (access.aspects.get(member), access.aspects.get(root))
        else {
            return;
        };
        let deferred = &mut *deferred;
        own.replicate
            .join(member, root_aspects.replicate.value, deferred);
        own.prediction
            .join(member, root_aspects.prediction.value, deferred);
        own.interpolation
            .join(member, root_aspects.interpolation.value, deferred);
        own.rooms.join(member, root_aspects.rooms.value, deferred);
        if let Some(bits) = access.bits.as_deref().copied()
            && !own.visibility_overridden
        {
            let holds_own = access.senders.iter().any(|visibility| {
                bits.hides(visibility, member) && !bits.agree(visibility, member, root)
            });
            if holds_own {
                deferred.insert(member, Overridden::<ManualVisibility>::default());
            } else {
                for mut visibility in &mut access.senders {
                    bits.copy(&mut visibility, member, root);
                }
            }
        }
    }

    fn reattach(trigger: On<'_, '_, Discard<ReplicateLike>>, mut commands: Commands<'_, '_>) {
        commands.run_system_cached_with(Self::reattached, trigger.entity);
    }

    /// A member whose attachment changes follows its new root. An entity that leaves its root keeps
    /// only what it holds, and the copies that [`OnDetach`] keeps (13-net-stack/replication.md, An
    /// entity joins with what it holds).
    fn reattached(
        In(member): In<Entity>,
        mut access: LinkAccess<'_, '_>,
        mut deferred: Deferred<'_, DeferredEntityCommands>,
    ) {
        let Ok(own) = access.aspects.get(member) else {
            return;
        };
        let deferred = &mut *deferred;
        let Ok(&ReplicateLike { root }) = access.likes.get(member) else {
            own.replicate.detach(member, deferred);
            own.prediction.detach(member, deferred);
            own.interpolation.detach(member, deferred);
            own.rooms.detach(member, deferred);
            if own.visibility_overridden {
                deferred.remove::<Overridden<ManualVisibility>>(member);
            }
            return;
        };
        let Ok(root_aspects) = access.aspects.get(root) else {
            return;
        };
        own.replicate
            .reattach(member, root_aspects.replicate.value, deferred);
        own.prediction
            .reattach(member, root_aspects.prediction.value, deferred);
        own.interpolation
            .reattach(member, root_aspects.interpolation.value, deferred);
        own.rooms
            .reattach(member, root_aspects.rooms.value, deferred);
        let visibility_overridden = own.visibility_overridden;
        if let Some(bits) = access.bits.as_deref().copied()
            && !visibility_overridden
        {
            for mut visibility in &mut access.senders {
                bits.copy(&mut visibility, member, root);
            }
        }
    }
}

impl Plugin for InheritancePlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(Self::join);
        app.add_observer(Self::reattach);
        app.add_observer(Self::visibility_override_removed);
        Self::register::<Replicate>(app);
        Self::register::<PredictionTarget>(app);
        Self::register::<InterpolationTarget>(app);
        Self::register::<Rooms>(app);
    }
}

#[cfg(test)]
mod tests;
