use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_actions::{ActionTarget, DeliveryShape};
use campfire_declare::{CallbackTable, Callbacks};
use campfire_math::Vec3;
use campfire_sim::Capability;

use crate::delivering::Launcher;

/// How a delivery of a shape sets out: the launcher's delivery, which aimed at the target from the
/// point.
pub(crate) type Deliver = fn(&mut World, Launcher, Vec3, ActionTarget);

/// How each shape of delivery sets out, by the capability that delivers it, which declares it, so
/// a cast that delivers names no capability's units.
#[derive(Resource, Debug, Default)]
pub struct Deliverers(CallbackTable<Deliver, { Capability::ALL.len() }>);

impl Callbacks for Deliverers {
    type Key = Capability;
    type Callback = Deliver;

    fn owner(capability: Capability) -> Option<Capability> {
        Some(capability)
    }

    fn register(&mut self, capability: Capability, deliver: Deliver) {
        self.0.register(capability as usize, deliver);
    }
}

impl Deliverers {
    /// How a delivery of `shape` sets out, which the match's capability of it declared.
    pub fn of(&self, shape: DeliveryShape) -> Deliver {
        self.0
            .get(shape.capability() as usize)
            .expect("the load refuses a delivery of a capability the match does not have")
    }
}
