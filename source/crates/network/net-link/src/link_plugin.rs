use crate::link_state::Unlinked;
use crate::link_systems::LinkSystems;
use crate::unlink::Unlink;
use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::prelude::*;

/// The link lifecycle and the sets the packet sessions run in: they hand
/// each link what they opened in [`LinkSystems::Receive`], and take what the frame queued in
/// [`LinkSystems::Send`].
#[derive(Debug)]
pub struct LinkPlugin;

impl LinkPlugin {
    /// An [`Unlink`] inserts [`Unlinked`].
    fn unlink(unlink: On<'_, '_, Unlink>, mut commands: Commands<'_, '_>) {
        commands.entity(unlink.entity).try_insert(Unlinked {
            reason: unlink.reason,
        });
    }
}

impl Plugin for LinkPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(PreUpdate, LinkSystems::Receive);
        app.configure_sets(PostUpdate, LinkSystems::Send);
        app.add_observer(Self::unlink);
    }
}
