use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::Position;

use crate::actions::action_target::ActionTarget;
use crate::actions::delivery::{Delivery, DeliveryShape};
use crate::deliveries::delivering::Delivering;

/// How a delivery of a shape sets out: the delivery of `by`, which aimed at the target from the
/// point, as `Delivery` gives it.
pub(crate) type Deliver = fn(&mut World, Delivering, Position, Delivery, ActionTarget);

/// How each shape of delivery sets out, which the capability of the shape registers as it
/// installs, so a cast that delivers names no capability's units. Not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct Deliverers {
    projectile: Option<Deliver>,
    area: Option<Deliver>,
}

impl Deliverers {
    /// Registers how projectiles set out.
    pub(crate) fn register_projectile(&mut self, deliver: Deliver) {
        assert!(self.projectile.is_none(), "projectiles register once");
        self.projectile = Some(deliver);
    }

    /// Registers how areas set out.
    pub(crate) fn register_area(&mut self, deliver: Deliver) {
        assert!(self.area.is_none(), "areas register once");
        self.area = Some(deliver);
    }

    /// How a delivery of `shape` sets out, which a match registered as it installed its
    /// capability.
    pub(crate) fn of(&self, shape: DeliveryShape) -> Deliver {
        let deliver = match shape {
            DeliveryShape::Projectile { .. } => self.projectile,
            DeliveryShape::Area => self.area,
        };
        deliver.expect("the load refuses a delivery of a capability the match does not have")
    }
}
