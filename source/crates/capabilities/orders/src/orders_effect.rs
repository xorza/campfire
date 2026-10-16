use bevy_ecs::world::World;
use campfire_script::{Effect, Frame};
use campfire_sim::SimWorld;
use campfire_units::MapBook;

use crate::resetting::Resetting;
use crate::unit_order::{OrderedUnit, UnitOrder};

/// An order an AI call queued for the unit that thinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OrdersEffect(pub(crate) UnitOrder);

/// For the unit that thinks in the call, which checked the order against the units as the phase
/// began; no unit dies within Think. A unit that resets takes none, and a reset marks the unit
/// as resetting until it is home.
impl Effect for OrdersEffect {
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        let id = frame
            .acting()
            .expect("an order comes from the unit that thinks");
        let entity = world.entity_of(id).expect("a unit that thinks lives");
        let bounds = world.resource::<MapBook>().bounds;
        let mut unit = world.entity_mut(entity);
        if unit.contains::<Resetting>() {
            return;
        }
        let parts = unit
            .get_components_mut::<OrderedUnit>()
            .expect("an ordered unit stands");
        self.0.give(parts, &bounds, now);
        if self.0 == UnitOrder::Reset {
            unit.insert(Resetting);
        }
    }
}
