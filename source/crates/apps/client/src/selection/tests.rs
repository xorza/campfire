use campfire_math::Num;
use campfire_orders::OrdersDirective;
use campfire_sim::IdAllocator;
use campfire_units::GroundPoint;

use super::*;

#[test]
fn a_pick_selects_its_units_alone_and_none_leaves_the_selection() {
    let mut ids = IdAllocator::default();
    let [a, b, c] = [ids.allocate(), ids.allocate(), ids.allocate()];
    let mut selection = Selection::default();
    // Out of order and twice over: each once, in id order.
    assert!(selection.pick(&[c, a, c], |_| None));
    assert_eq!(selection.get(), [a, c]);
    // An empty pick, as a drag over none of the player's units, leaves it.
    assert!(!selection.pick(&[], |_| None));
    assert_eq!(selection.get(), [a, c]);
    assert!(selection.pick(&[b], |_| None));
    assert_eq!(selection.get(), [b]);
    // A pick of c, a spawn of the leader a, and of b selects a and b: the spawn as its leader
    // (02-engine-core.md, A member picks as its leader).
    assert!(selection.pick(&[c, b], |unit| (unit == c).then_some(a)));
    assert_eq!(selection.get(), [a, b]);
}

#[test]
fn a_unit_the_client_no_longer_holds_leaves_and_an_empty_selection_takes_the_avatar() {
    let mut ids = IdAllocator::default();
    let [avatar, tank, dozer] = [ids.allocate(), ids.allocate(), ids.allocate()];
    let mut selection = Selection::default();
    // Before the avatar spawns, or in a mode with none, an empty selection stays empty.
    assert!(!selection.keep(|_| true, None));
    assert_eq!(selection.get(), []);
    // Once it spawns, the empty selection takes it.
    assert!(selection.keep(|_| true, Some(avatar)));
    assert_eq!(selection.get(), [avatar]);
    // A selection of units all held keeps them, the avatar not among them.
    selection.pick(&[tank, dozer], |_| None);
    assert!(!selection.keep(|_| true, Some(avatar)));
    assert_eq!(selection.get(), [tank, dozer]);
    // The tank gone: the dozer alone.
    assert!(selection.keep(|unit| unit != tank, Some(avatar)));
    assert_eq!(selection.get(), [dozer]);
    // Both gone: the avatar, and with no avatar, none.
    let mut other = Selection::default();
    other.pick(&[tank, dozer], |_| None);
    assert!(selection.keep(|_| false, Some(avatar)));
    assert_eq!(selection.get(), [avatar]);
    assert!(other.keep(|_| false, None));
    assert_eq!(other.get(), []);
}

#[test]
fn a_selection_orders_its_units_as_one_group() {
    let mut ids = IdAllocator::default();
    let [a, b] = [ids.allocate(), ids.allocate()];
    let action = OrdersDirective::Move {
        to: GroundPoint {
            x: Num::int(3),
            z: Num::int(-2),
        },
    };
    let mut selection = Selection::default();
    assert_eq!(selection.order(action), None);
    selection.pick(&[b, a], |_| None);
    let order = selection.order(action).unwrap();
    assert_eq!(order.units.get(), [a, b]);
    assert_eq!(order.directive, Directive::from(action));
    selection.pick(&[b], |_| None);
    assert_eq!(
        selection.order(OrdersDirective::Stop),
        Some(Order::one(b, OrdersDirective::Stop))
    );
}
