use crate::link_state::Unlinked;
use crate::link_systems::LinkSystems;
use crate::unlink::Unlink;
use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::prelude::*;
use campfire_net_timeline::ClockPlugin;

/// Bevy plugin that installs the link lifecycle and the sets the sessions run in: they hand each
/// link what they opened in [`LinkSystems::Receive`], and take what the frame queued in
/// [`LinkSystems::Send`].
#[derive(Debug)]
pub struct LinkPlugin;

impl LinkPlugin {
    /// Handles [`Unlink`] requests by inserting [`Unlinked`].
    fn unlink(unlink: On<'_, '_, Unlink>, mut commands: Commands<'_, '_>) {
        commands.entity(unlink.entity).try_insert(Unlinked {
            reason: unlink.reason,
        });
    }
}

impl Plugin for LinkPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ClockPlugin>() {
            app.add_plugins(ClockPlugin);
        }
        app.configure_sets(PreUpdate, LinkSystems::Receive);
        app.configure_sets(PostUpdate, LinkSystems::Send);
        app.add_observer(Self::unlink);
    }
}
