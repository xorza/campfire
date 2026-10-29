use bevy_app::{App, Plugin};

use crate::app_filter_ext::AppFilterExt;
use crate::controlled_by::ControlledBy;

#[derive(Debug)]
pub(crate) struct ControlPlugin;

impl Plugin for ControlPlugin {
    fn build(&self, app: &mut App) {
        // ControlledSend is registered in SharedComponentRegistrationPlugin
        // so its wire ID and custom Controlled receive behavior match.
        app.add_filter::<ControlledBy>();
        app.add_observer(ControlledBy::end_with_link);
    }
}

#[cfg(test)]
mod tests;
