use bevy_ecs::resource::Resource;
use campfire_core::{CallError, Callbacks, Frame, View};
use campfire_sim::{Capability, StableId};

use crate::capability_does_spec::CapabilityDoesSpec;

/// How a listed effect of a capability above the action pipeline queues in a call: its `does`
/// to `unit`, of the list that reached `reached`, if a unit, in the call `frame` holds, which
/// `view` sees.
pub type QueueListed =
    fn(CapabilityDoesSpec, StableId, Option<StableId>, &mut Frame, &View) -> Result<(), CallError>;

/// How the listed effects of each capability above the action pipeline queue, by capability:
/// each declares its own, as a call's effects apply themselves, so the lists name no
/// capability's effect types.
#[derive(Resource, Debug, Default)]
pub struct EffectQueues([Option<QueueListed>; Capability::ALL.len()]);

impl Callbacks for EffectQueues {
    type Key = Capability;
    type Callback = QueueListed;

    fn owner(capability: Capability) -> Option<Capability> {
        Some(capability)
    }

    fn register(&mut self, capability: Capability, queue: QueueListed) {
        let slot = &mut self.0[capability as usize];
        assert!(slot.is_none(), "{capability:?} registers its effects once");
        *slot = Some(queue);
    }
}

impl EffectQueues {
    /// How `capability`'s listed effects queue, which a match registered as it installed it.
    pub fn of(&self, capability: Capability) -> QueueListed {
        self.0[capability as usize]
            .expect("the load refuses an effect of a capability the match does not have")
    }
}
