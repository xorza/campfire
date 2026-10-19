use crate::controlled::Controlled;
use crate::controlled_by::ControlledBy;
use crate::controlled_by::ControlledByRemote;
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::AppVisibilityExt;
use campfire_net_connection::HostClient;

/// On a host server, the server's world is the host client's: an entity a host client controls
/// is `Controlled` there, exactly while that client is the host client and controls it.
fn sync_controlled(
    trigger: On<'_, '_, Insert<ControlledBy>>,
    controlled_by: Query<'_, '_, &ControlledBy>,
    hosts: Query<'_, '_, (), With<HostClient>>,
    mut commands: Commands<'_, '_>,
) {
    let Ok(controlled_by) = controlled_by.get(trigger.entity) else {
        return;
    };
    let mut entity = commands.entity(trigger.entity);
    if hosts.contains(controlled_by.link) {
        entity.try_insert(Controlled);
    } else {
        entity.try_remove::<Controlled>();
    }
}

fn remove_controlled(trigger: On<'_, '_, Remove<ControlledBy>>, mut commands: Commands<'_, '_>) {
    if trigger.trigger().new_archetype.is_some() {
        commands.entity(trigger.entity).try_remove::<Controlled>();
    }
}

fn controlled_on_host_added(
    trigger: On<'_, '_, Add<HostClient>>,
    owned: Query<'_, '_, &ControlledByRemote>,
    mut commands: Commands<'_, '_>,
) {
    if let Ok(owned) = owned.get(trigger.entity) {
        for entity in owned.collection() {
            commands.entity(*entity).try_insert(Controlled);
        }
    }
}

fn controlled_on_host_removed(
    trigger: On<'_, '_, Remove<HostClient>>,
    owned: Query<'_, '_, &ControlledByRemote>,
    mut commands: Commands<'_, '_>,
) {
    if let Ok(owned) = owned.get(trigger.entity) {
        for entity in owned.collection() {
            commands.entity(*entity).try_remove::<Controlled>();
        }
    }
}

#[derive(Debug)]
pub(crate) struct ControlPlugin;

impl Plugin for ControlPlugin {
    fn build(&self, app: &mut App) {
        // `ControlledBy` is a `VisibilityFilter`: Replicon evaluates it on
        // insertion and on link changes, replacing the previous manual writes.
        // ControlledSend is registered in SharedComponentRegistrationPlugin
        // so its wire ID and custom Controlled receive behavior match.
        app.add_visibility_filter::<ControlledBy>();
        app.add_observer(ControlledBy::end_link_session);
        app.add_observer(sync_controlled);
        app.add_observer(remove_controlled);
        app.add_observer(controlled_on_host_added);
        app.add_observer(controlled_on_host_removed);
    }
}

#[cfg(test)]
mod tests;
