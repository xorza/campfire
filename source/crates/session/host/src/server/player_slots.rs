use bevy_ecs::entity::Entity;
use bevy_ecs::system::Commands;
use campfire_protocol::Delegation;
use campfire_values::TeamSet;

use crate::error::JoinError;

/// Where a frame's checked joins go, as `CheckedJoins::admit` takes them: the lobby's player_slots
/// before the start, the door's after it.
pub trait PlayerSlots<'w, 's> {
    /// The commands a refusal goes through.
    fn commands(&mut self) -> &mut Commands<'w, 's>;

    /// Admits the spectator of `link`, which chose to watch `teams`, while a place is free.
    fn watch(&mut self, link: Entity, teams: TeamSet) -> Result<(), JoinError>;

    /// Places the player of `link`, whom `delegation` names.
    fn player_slot(&mut self, link: Entity, delegation: Delegation) -> Result<(), JoinError>;
}
