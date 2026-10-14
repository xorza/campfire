use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_actions::{ActionTarget, Delivery, DeliveryShape};
use campfire_declare::Callbacks;
use campfire_sim::{Capability, Position};

use crate::delivering::Delivering;

/// How a delivery of a shape sets out: the delivery of `by`, which aimed at the target from the
/// point, as `Delivery` gives it.
pub(crate) type Deliver = fn(&mut World, Delivering, Position, Delivery, ActionTarget);

campfire_common::enum_all! {
    /// A kind of delivery that sets out: a key of `Deliverers`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DeliveryKind {
        Projectile,
        Area,
    }
    pub const ALL;
}

/// How each kind of delivery sets out, which the capability of the kind declares, so a cast that
/// delivers names no capability's units.
#[derive(Resource, Debug, Default)]
pub struct Deliverers([Option<Deliver>; DeliveryKind::ALL.len()]);

impl Callbacks for Deliverers {
    type Key = DeliveryKind;
    type Callback = Deliver;

    fn owner(kind: DeliveryKind) -> Option<Capability> {
        match kind {
            DeliveryKind::Projectile => Some(Capability::Projectiles),
            DeliveryKind::Area => Some(Capability::Areas),
        }
    }

    fn register(&mut self, kind: DeliveryKind, deliver: Deliver) {
        let slot = &mut self.0[kind as usize];
        assert!(slot.is_none(), "{kind:?} registers how it sets out once");
        *slot = Some(deliver);
    }
}

impl Deliverers {
    /// How a delivery of `shape` sets out, which the match's capability of it declared.
    pub fn of(&self, shape: DeliveryShape) -> Deliver {
        let kind = match shape {
            DeliveryShape::Projectile { .. } => DeliveryKind::Projectile,
            DeliveryShape::Area => DeliveryKind::Area,
        };
        self.0[kind as usize]
            .expect("the load refuses a delivery of a capability the match does not have")
    }
}
