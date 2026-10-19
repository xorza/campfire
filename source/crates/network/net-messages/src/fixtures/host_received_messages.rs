use bevy_app::{App, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::{Query, ResMut, Resource};

use crate::message::Message;
use crate::message_receiver::HostMessageReceiver;

/// Every message `M` a host client's own side received, with the host client entity, in arrival
/// order: what its server sent it, which
/// [`ReceivedMessages`](crate::fixtures::received_messages::ReceivedMessages) does not hold.
#[derive(Debug, Resource)]
pub struct HostReceivedMessages<M>(pub Vec<(Entity, M)>);

impl<M> Default for HostReceivedMessages<M> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<M: Message> HostReceivedMessages<M> {
    /// Collects every message `M` that `app`'s host client receives from now on.
    pub fn collect_in(app: &mut App) {
        app.init_resource::<Self>()
            .add_systems(Update, Self::collect);
    }

    fn collect(
        mut receivers: Query<'_, '_, (Entity, &mut HostMessageReceiver<M>)>,
        mut received: ResMut<'_, Self>,
    ) {
        for (entity, mut receiver) in &mut receivers {
            received
                .0
                .extend(receiver.receive().map(|message| (entity, message)));
        }
    }
}
