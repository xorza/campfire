use bevy_ecs::world::World;
use campfire_actions::ProgressDues;
use campfire_navigation::RouteDues;
use campfire_script::{Effect, EffectKind, Frame};
use campfire_sim::SimWorld;
use campfire_units::{MapBook, UnitOrdered};

use crate::attack_move::GoalDues;
use crate::resetting::Resetting;
use crate::unit_order::{OrderedUnit, UnitDues, UnitOrder};

/// An order an AI call queued for the unit that thinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OrdersEffect(pub(crate) UnitOrder);

/// For the unit that thinks in the call, which checked the order against the units as the phase
/// began; no unit dies within Think. A unit that resets takes none, and a reset marks the unit
/// as resetting until it is home.
impl Effect for OrdersEffect {
    const KIND: EffectKind = EffectKind::Order;

    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        let id = frame
            .acting()
            .expect("an order comes from the unit that thinks");
        let entity = world.entity_of(id).expect("a unit that thinks lives");
        let bounds = world.resource::<MapBook>().bounds;
        if world.entity(entity).contains::<Resetting>() {
            return;
        }
        let order = ProgressDues::with(world, id, entity, |world, progress| {
            RouteDues::with(world, id, entity, |world, routes| {
                GoalDues::with(world, id, entity, |world, goals| {
                    let mut unit = world.entity_mut(entity);
                    let parts = unit
                        .get_components_mut::<OrderedUnit>()
                        .expect("an ordered unit stands");
                    let mut dues = UnitDues {
                        progress: progress.reborrow(),
                        routes: routes.reborrow(),
                        goals: goals.reborrow(),
                    };
                    self.0.give(parts, &bounds, now, &mut dues)
                })
            })
        });
        let Some(order) = order else {
            return;
        };
        if self.0 == UnitOrder::Reset {
            world.entity_mut(entity).insert(Resetting);
        }
        world.trigger(UnitOrdered { entity, order });
    }
}
