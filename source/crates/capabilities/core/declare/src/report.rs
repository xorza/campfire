use bevy_ecs::resource::Resource;
use campfire_script::{ScriptBudgets, ScriptPrints};

use crate::role::Role;

/// A tick's record for the host, which the sim writes within a tick and the host reads after it:
/// no state, as the sim never reads it from an earlier tick.
pub trait Report: Resource + Sized {
    /// The report as the install of a match of `role` puts it in; none for a role that holds
    /// none, or for one its module puts in as an event asks.
    fn installed(role: &Role) -> Option<Self>;
}

/// What each pool spent in the last tick, which a server's match alone holds, as it runs scripts;
/// full again as the next tick starts.
impl Report for ScriptBudgets {
    fn installed(role: &Role) -> Option<Self> {
        role.pools().map(Self::full)
    }
}

/// What the scripts printed in the last tick, which a match that runs scripts alone holds.
impl Report for ScriptPrints {
    fn installed(role: &Role) -> Option<Self> {
        role.scripts().then(Self::default)
    }
}
