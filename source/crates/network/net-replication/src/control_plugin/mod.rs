use bevy_app::{App, Plugin};
use bevy_replicon::prelude::AppVisibilityExt;

use crate::controlled_by::ControlledBy;

#[derive(Debug)]
pub(crate) struct ControlPlugin;

impl Plugin for ControlPlugin {
    fn build(&self, app: &mut App) {
        // `ControlledBy` is a `VisibilityFilter`: Replicon evaluates it on
        // insertion and on link changes, replacing the previous manual writes.
        // ControlledSend is registered in SharedComponentRegistrationPlugin
        // so its wire ID and custom Controlled receive behavior match.
        app.add_visibility_filter::<ControlledBy>();
        app.add_observer(ControlledBy::end_with_link);
    }
}

#[cfg(test)]
mod tests;
