use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use campfire_net_payload::UnlinkReason;

use crate::link::Link;

/// An established link; its insert removes [`Linking`] and [`Unlinked`].
#[derive(Component, Default, Debug)]
#[component(on_insert = Linked::on_insert)]
pub struct Linked;

impl Linked {
    fn on_insert(mut world: DeferredWorld<'_>, context: HookContext) {
        world
            .commands()
            .entity(context.entity)
            .try_remove::<(Linking, Unlinked)>();
    }
}

/// A link being established; its insert removes [`Linked`] and [`Unlinked`]. On a link that is
/// already [`Linked`], as when an IO links in the same flush that started the link, the insert is
/// undone instead: a completed link does not regress to the in-progress state.
#[derive(Component, Default, Debug)]
#[component(on_insert = Linking::on_insert)]
pub struct Linking;

impl Linking {
    fn on_insert(mut world: DeferredWorld<'_>, context: HookContext) {
        if world.get::<Linked>(context.entity).is_some() {
            world.commands().entity(context.entity).try_remove::<Self>();
            return;
        }
        world
            .commands()
            .entity(context.entity)
            .try_remove::<(Linked, Unlinked)>();
    }
}

/// A link that is not connected. Its insert removes [`Linked`] and [`Linking`], and empties the
/// link's packet session queues: what they hold is the last packet session's, and the next packet
/// session would read it as its own, as the fate of a payload it never sent. The
/// [`reason`](Self::reason) is intended for diagnostics and for transports that need to surface
/// disconnect causes to application code (11-net-stack/connection.md, An unlinked link holds
/// nothing of its last connection).
#[derive(Component, Default, Debug)]
#[component(on_insert = Unlinked::on_insert)]
pub struct Unlinked {
    /// Structured disconnect or initial-state reason.
    pub reason: UnlinkReason,
}

impl Unlinked {
    fn on_insert(mut world: DeferredWorld<'_>, context: HookContext) {
        if let Some(mut link) = world.get_mut::<Link>(context.entity) {
            link.packet_session.clear();
        }
        world
            .commands()
            .entity(context.entity)
            .try_remove::<(Linked, Linking)>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_payload::OutgoingPayload;
    use campfire_net_payload::PacketNumber;
    use campfire_net_payload::PayloadKind;
    use campfire_net_payload::PayloadToken;

    /// A link that unlinks keeps nothing of its packet session: no budget and no queued send or
    /// accept. Another entity's link keeps its own.
    #[test]
    fn an_unlinked_link_holds_nothing_of_its_last_connection() {
        let mut world = World::new();
        let queued = || {
            let mut link = Link::default();
            link.packet_session.set_room(100, 0);
            link.packet_session.send(
                PayloadToken(1),
                PayloadKind::new(0),
                OutgoingPayload::buffer(&[]),
            );
            link.packet_session.accept(PacketNumber(2));
            link
        };
        let entity = world.spawn((queued(), Linked)).id();
        let other = world.spawn((queued(), Linked)).id();

        world.entity_mut(entity).insert(Unlinked::default());
        let mut link = world.get_mut::<Link>(entity).unwrap();
        assert_eq!(link.packet_session.budget(), 0);
        assert_eq!(link.packet_session.take_sends().count(), 0);
        assert_eq!(link.packet_session.take_accepted().count(), 0);
        let mut other = world.get_mut::<Link>(other).unwrap();
        assert_eq!(other.packet_session.budget(), 100);
        assert_eq!(other.packet_session.take_sends().count(), 1);
        assert_eq!(other.packet_session.take_accepted().count(), 1);
    }
}
