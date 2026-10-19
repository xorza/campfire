use bevy_ecs::event::Event;

/// The state was written from outside the sim's own systems: a restore ended, a client's rollback
/// restored the parts it predicts, or a match installed over units a test spawned first. Each
/// cache that follows the state's changes builds again from the state.
#[derive(Event, Debug, Clone, Copy)]
pub struct StateRewritten;
