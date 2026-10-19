use bevy_app::{App, Update};
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::{Query, ResMut, Resource};

use crate::message::Message;
use crate::message_receiver::MessageReceiver;

/// Every message `M` an app received, with the link entity it arrived on, in arrival order.
///
/// A receiver drops the messages left unread at the end of its frame, so a test collects them in
/// the frame with [`Self::collect_in`].
#[derive(Debug, Resource)]
pub struct ReceivedMessages<M>(pub Vec<(Entity, M)>);

impl<M> Default for ReceivedMessages<M> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<M: Message> ReceivedMessages<M> {
    /// Collects every message `M` that `app` receives from now on.
    pub fn collect_in(app: &mut App) {
        app.init_resource::<Self>()
            .add_systems(Update, Self::collect);
    }

    fn collect(
        mut receivers: Query<'_, '_, (Entity, &mut MessageReceiver<M>)>,
        mut received: ResMut<'_, Self>,
    ) {
        for (entity, mut receiver) in &mut receivers {
            received
                .0
                .extend(receiver.receive().map(|message| (entity, message)));
        }
    }
}
