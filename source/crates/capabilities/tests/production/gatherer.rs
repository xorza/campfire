use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_production::*;
use campfire_script::View;
use campfire_sim::{IdAllocator, SimComponent, TickRate};
use campfire_values::{Names, ResourceId};

#[test]
fn a_restored_gatherer_carries_a_known_resource_with_a_node_to_go_back_to() {
    // A match of gold alone: wood, the second of two resources, is not its own.
    let mut world = World::new();
    // The world holds no units for the view to read.
    let view = View::new(TickRate::new(NonZeroU32::new(30).unwrap()), |_| {});
    view.names_mut()
        .set_table(Arc::new(Names::<ResourceId>::from(["gold"])));
    world.insert_non_send(view);
    let entity = world.spawn_empty().id();
    let [gold, wood] = [0, 1].map(ResourceId::new);
    let mut ids = IdAllocator::default();
    let [first, second] = [0, 1].map(|z| NodeAt {
        node: ids.allocate(),
        pos: Vec3::new(Num::ZERO, Num::ZERO, Num::int(z)),
    });
    let load = |resource| Load {
        resource,
        amount: 5,
    };
    let order = |node| GatherOrder {
        slot: 0,
        node,
        step: GatherStep::ToNode,
    };
    let checks = |order, load, last| Gatherer::of_parts(order, load, last).check(&world, entity);
    assert!(checks(None, None, None));
    assert!(checks(None, Some(load(gold)), Some(first)));
    assert!(checks(Some(order(first)), None, Some(first)));
    assert!(!checks(None, Some(load(wood)), Some(first)));
    assert!(!checks(None, Some(load(gold)), None));
    assert!(!checks(Some(order(first)), None, Some(second)));
    assert!(!checks(Some(order(first)), None, None));
    // An order not checked yet names any unit, and keeps the last node it chose.
    let ordered = GatherOrder {
        step: GatherStep::Ordered,
        ..order(second)
    };
    assert!(checks(Some(ordered), Some(load(gold)), Some(first)));
    let mut gatherer = Gatherer::default();
    gatherer.set(Some(order(first)));
    gatherer.set(Some(ordered));
    assert_eq!(gatherer.last(), Some(first));
    gatherer.set(Some(order(second)));
    assert_eq!(gatherer.last(), Some(second));
}
