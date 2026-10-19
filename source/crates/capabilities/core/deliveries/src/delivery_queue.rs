use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use campfire_actions::Reached;
use campfire_declare::{Role, Scratch};
use campfire_math::Vec3;
use campfire_sim::StableId;
use campfire_values::Hit;

use crate::dash_delivering::DashDelivering;
use crate::delivered::{Delivered, DeliveredPayload};

/// The hits and ends of this tick's deliveries whose action's hooks run, in the order they
/// happened; and the delivery units that ended, which despawn once the hooks ran, so a hook still
/// reads its delivery. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct DeliveryQueue {
    pub(crate) delivered: Vec<Delivered>,
    pub(crate) ended: Vec<Entity>,
}

/// The end of a dash that delivers `dash`, aimed at `target` if a unit, whose unit stands at
/// `pos` after a last step in `direction`.
#[derive(Debug, Clone, Copy)]
pub struct DashEnd {
    pub dash: DashDelivering,
    pub target: Option<StableId>,
    pub pos: Vec3,
    pub direction: Option<Vec3>,
}

impl DeliveryQueue {
    /// Records `delivered`, a delivery that reached what it reached this tick.
    pub fn deliver(&mut self, delivered: Delivered) {
        self.delivered.push(delivered);
    }

    /// What the tick's deliveries reached so far, in the order they did.
    pub fn delivered(&self) -> &[Delivered] {
        &self.delivered
    }

    /// Records that the delivery unit `entity` ended this tick, so it despawns.
    pub fn end(&mut self, entity: Entity) {
        self.ended.push(entity);
    }

    /// Records `end`, the end of a dash that delivers an action: an end with no delivery unit,
    /// after the meters the dash went, whose hooks are its action's.
    pub fn end_dash(&mut self, end: DashEnd) {
        let DashEnd {
            dash,
            target,
            pos,
            direction,
        } = end;
        let payload = DeliveredPayload::Action(dash.by);
        let hit = Hit {
            delivery: None,
            target,
            pos,
            distance: dash.dashed,
            direction,
        };
        self.delivered.push(Delivered {
            payload,
            reach: Reached::End,
            hit,
        });
    }
}

impl Scratch for DeliveryQueue {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.delivered.is_empty() && self.ended.is_empty()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::delivered::Delivered;
    use crate::delivery_queue::DeliveryQueue;

    impl DeliveryQueue {
        /// The deliveries recorded this tick, to change as a test sets them.
        pub const fn delivered_mut(&mut self) -> &mut Vec<Delivered> {
            &mut self.delivered
        }
    }
}
