use bevy_ecs::resource::Resource;
use campfire_common::Ticks;

/// How long after its last strike an attacker counts as assisting in a death: the mode's
/// `[combat] assist_window_ms`, in ticks. Package data, not state; without it, no one assists.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssistWindow(pub(crate) Ticks);
