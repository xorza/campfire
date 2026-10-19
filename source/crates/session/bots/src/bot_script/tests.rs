use campfire_actions::{ActionBook, ActionTarget};
use campfire_capabilities::Books;
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_orders::OrdersAction;
use campfire_script::UnknownName;
use campfire_sim::IdAllocator;
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{MapPointError, UnitTypeBook};
use campfire_values::{ActionId, FilterData};

use super::*;

/// The books of the lane mode, its map with a marker of a region, `camp`, beside its spawns.
fn lane_books() -> Books {
    let camp = "[[markers]]\nname = \"camp\"\ntags = [\"camp\"]\n\
                region = { min = [-4, -4], max = [-2, -2] }\n";
    PackageTree::of(TestMode::Lane)
        .edit("modes/lane/map/lane/map.toml", Edit::Append(camp))
        .packages()
        .name_books()
}

#[test]
fn a_bot_takes_its_inputs_and_its_orders_apart_each_once() {
    let script = OrderScript::parse(
        "[[input]]\ntick = 0\nname = \"hero\"\nvalue = \"hero-x\"\n\
         [[order]]\ntick = 2\naction.orders.move.to = [1, 0]\n\
         [[order]]\ntick = 5\naction.orders.move.to = [0, 1]\n",
    )
    .unwrap();
    let ticks =
        |orders: &[BotOrder]| -> Vec<u64> { orders.iter().map(|order| order.tick.get()).collect() };
    let books = lane_books();
    let mut bot = BotScript::new(&script, &BotNames::new(&books)).unwrap();
    let mut late = bot.clone();
    // A client with no avatar yet takes the inputs only: the orders due by tick 3 wait.
    assert_eq!(bot.due_inputs(Tick::new(3)).len(), 1);
    assert_eq!(bot.due_inputs(Tick::new(3)), []);
    // Taken late, at tick 6, the orders due by then go out together, each once; the order of
    // tick 5, not ready at tick 6, waits, and the one of tick 2 goes alone.
    let not_five = |order: &BotOrder| order.tick.get() != 5;
    assert_eq!(ticks(late.due_orders_while(Tick::new(6), not_five)), [2]);
    assert_eq!(ticks(late.due_orders(Tick::new(6))), [5]);
    assert_eq!(ticks(bot.due_orders(Tick::new(6))), [2, 5]);
    assert!(bot.due_orders(Tick::new(9)).is_empty());
}

#[test]
fn a_bot_resolves_each_name_of_its_script_as_it_starts_or_refuses_the_first_the_match_lacks() {
    let books = lane_books();
    let names = BotNames::new(&books);
    let bot = |text: &str| {
        let text = format!("[[order]]\ntick = 0\n{text}\n");
        BotScript::new(&OrderScript::parse(&text).unwrap(), &names)
    };
    let first = |bot: &BotScript| bot.orders[0].clone();
    // The walker's package and the runner's each name an action `first`: a slot of that name is
    // either, as each hero holds its own.
    let actions = books.get::<ActionBook>().unwrap().names();
    let firsts: Vec<ActionId> = actions
        .iter()
        .filter(|&(_, name)| name == "first")
        .map(|(action, _)| action)
        .collect();
    assert_eq!(firsts.len(), 2);
    let enemies = FilterData::parse("enemies:avatar").unwrap();
    let tags = books.get::<UnitTypeBook>().unwrap().tags();
    let cast = first(
        &bot("action.orders.slot = { slot = \"first\", target.unit = \"enemies:avatar\" }")
            .unwrap(),
    );
    assert_eq!(cast.slot, Some(ActionName(firsts.clone().into())));
    assert_eq!(
        cast.action,
        Action::Orders(OrdersAction::Slot {
            slot: ActionName(firsts.into()),
            target: ActionTarget::Unit(Filter::resolve(&enemies, tags).unwrap()),
        })
    );
    // A marker's point, the east spawn's at (0, 2), and a point of the map; neither names a slot.
    let walk = |text| first(&bot(text).unwrap());
    let ground = |x, z| GroundPoint::new(Num::int(x), Num::int(z));
    let east = walk("action.orders.move.to.marker = \"east_spawn\"");
    let point = walk("action.orders.move.to = [3, -4]");
    for (order, to) in [(&east, ground(0, 2)), (&point, ground(3, -4))] {
        assert_eq!(order.action, Action::Orders(OrdersAction::Move { to }));
        assert_eq!(order.slot, None);
    }
    // The first name the match lacks fails the start: a unit type, an action, a tag of a filter,
    // a marker, and an item type of a mode with no items; a marker of a region has no point, and
    // a point past the world's bound, or of a spatial map's shape, fits no planar map. One off
    // the map, past x = 20, is an order the sim brings onto it.
    let unknown = |text: &str| match bot(text).unwrap_err() {
        BotScriptError::Unknown(UnknownName { of, name }) => (of.to_string(), name),
        error => panic!("{error:?}"),
    };
    let stop = "action.orders = \"stop\"";
    let named = |of: &str, name: &str| (of.to_owned(), name.to_owned());
    for (text, refused) in [
        (
            format!("units = \"tank\"\n{stop}"),
            named("unit type", "tank"),
        ),
        (
            "action.progression.learn.slot = \"nope\"".to_owned(),
            named("action", "nope"),
        ),
        (
            "action.orders.attack.target = \"enemies:flying\"".to_owned(),
            named("filter", "enemies:flying"),
        ),
        (
            "action.orders.move.to.marker = \"nowhere\"".to_owned(),
            named("marker", "nowhere"),
        ),
        (
            "action.items.buy = { item = \"potion\", shop = \"allies\" }".to_owned(),
            named("item type", "potion"),
        ),
    ] {
        assert_eq!(unknown(&text), refused, "{text}");
    }
    let refused = |text| bot(text).unwrap_err();
    assert_eq!(
        refused("action.orders.move.to.marker = \"camp\""),
        BotScriptError::NoPoint(DeclaredName::new("camp").unwrap())
    );
    let past = format!(
        "action.orders.move.to = [\"{}\", 0]",
        Vec3::BOUND + Num::EPSILON
    );
    assert_eq!(
        refused(&past),
        BotScriptError::Point(MapPointError::OutOfBounds)
    );
    assert_eq!(
        walk("action.orders.move.to = [21, 0]").action,
        Action::Orders(OrdersAction::Move { to: ground(21, 0) })
    );
    assert_eq!(
        refused("action.orders.move.to = [1, 0, 1]"),
        BotScriptError::Point(MapPointError::Shape)
    );
}

#[test]
fn a_due_order_of_a_slot_goes_to_its_units_by_the_place_each_holds_the_action_at() {
    let mut ids = IdAllocator::default();
    let [a, b, c, d] = [(); 4].map(|()| ids.allocate());
    // `a` and `c` hold the action at 1, `b` at 0, `d` not at all: the order of 0 goes to `b`,
    // that of 1 to `a` and `c`, in stable-id order, and `d` is missing.
    let place = |id| match id {
        _ if id == a || id == c => Some(1),
        _ if id == b => Some(0),
        _ => None,
    };
    let mut slotted = Vec::new();
    assert!(by_slot(&[a, b, c, d], place, &mut slotted));
    assert_eq!(slotted, [(0, b), (1, a), (1, c)]);
    let runs: Vec<Vec<StableId>> = slotted
        .chunk_by(|x, y| x.0 == y.0)
        .map(|run| run.iter().map(|&(_, id)| id).collect())
        .collect();
    assert_eq!(runs, [vec![b], vec![a, c]]);
    // With `d` left out, none is missing; with no unit, none either, and no order.
    assert!(!by_slot(&[c, a, b], place, &mut slotted));
    assert_eq!(slotted, [(0, b), (1, a), (1, c)]);
    assert!(!by_slot(&[], place, &mut slotted));
    assert_eq!(slotted, []);
}
