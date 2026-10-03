use bevy_ecs::resource::Resource;
use campfire_sim::{Capability, StableId};

use crate::actions::effect_lists::Does;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::units::script_view::View;

/// How a listed effect of a capability above the action pipeline queues in a call: its `does`
/// to `unit`, of the list that reached `reached`, if a unit, in the call `frame` holds, which
/// `view` sees.
pub(crate) type QueueListed =
    fn(Does, StableId, Option<StableId>, &mut Frame, &View) -> Result<(), CallError>;

/// How the listed effects of each capability above the action pipeline queue, by capability:
/// each registers its own as it installs, as a call's effects apply themselves, so the lists
/// name no capability's effect types. Not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct EffectQueues([Option<QueueListed>; Capability::ALL.len()]);

impl EffectQueues {
    /// Registers how `capability`'s listed effects queue.
    pub(crate) fn register(&mut self, capability: Capability, queue: QueueListed) {
        let slot = &mut self.0[capability as usize];
        assert!(slot.is_none(), "{capability:?} registers its effects once");
        *slot = Some(queue);
    }

    /// How `capability`'s listed effects queue, which a match registered as it installed it.
    pub(crate) fn of(&self, capability: Capability) -> QueueListed {
        self.0[capability as usize]
            .expect("the load refuses an effect of a capability the match does not have")
    }
}
