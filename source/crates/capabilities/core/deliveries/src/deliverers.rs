use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_actions::{ActionTarget, DeliveryShape};
use campfire_declare::{CallbackTable, Callbacks};
use campfire_derive::EnumAll;
use campfire_math::Vec3;
use campfire_sim::Capability;

use crate::delivering::Launcher;

/// How a delivery of a shape sets out: the launcher's delivery, which aimed at the target from the
/// point.
pub(crate) type Deliver = fn(&mut World, Launcher, Vec3, ActionTarget);

/// A kind of delivery that sets out: a key of `Deliverers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum DeliveryKind {
    Projectile,
    Area,
}

/// How each kind of delivery sets out, which the capability of the kind declares, so a cast that
/// delivers names no capability's units.
#[derive(Resource, Debug, Default)]
pub struct Deliverers(CallbackTable<Deliver, { DeliveryKind::ALL.len() }>);

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
        self.0.register(kind as usize, deliver);
    }
}

impl Deliverers {
    /// How a delivery of `shape` sets out, which the match's capability of it declared.
    pub fn of(&self, shape: DeliveryShape) -> Deliver {
        let kind = match shape {
            DeliveryShape::Projectile { .. } => DeliveryKind::Projectile,
            DeliveryShape::Area => DeliveryKind::Area,
        };
        self.0
            .get(kind as usize)
            .expect("the load refuses a delivery of a capability the match does not have")
    }
}
