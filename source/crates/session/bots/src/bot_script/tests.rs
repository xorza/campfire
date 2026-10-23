use std::num::{NonZeroU8, NonZeroU32};
use std::sync::Arc;

use campfire_actions::{ActionBook, ActionTarget};
use campfire_capabilities::Books;
use campfire_common::DeclaredName;
use campfire_items::{Inventory, ItemStack};
use campfire_mode::InputType;
use campfire_orders::OrdersDirective;
use campfire_script::UnknownName;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{IdAllocator, SimWorld};
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_units::{BuildTarget, Life, MapPointError, UnitTypeBook};
use campfire_values::{ActionId, FilterData};

use campfire_harness::{FixedMatch, FixedSession, MatchUnits};

use super::*;
use crate::bot_view::HeldBotView;

/// The books of the lane mode, its map with a marker of a region, `camp`, beside its spawns.
fn lane_books() -> Arc<Books> {
    let camp = "[[markers]]\nname = \"camp\"\ntags = [\"camp\"]\n\
                region = { min = [-4, -4], max = [-2, -2] }\n";
    let packages = PackageTree::of(TestMode::Lane)
        .edit("modes/lane/map/lane/map.toml", Edit::Append(camp))
        .packages();
    Arc::clone(packages.load_books())
}

#[test]
fn a_bot_takes_its_inputs_and_its_orders_apart_each_once() {
    let script = OrderScript::parse(
        "[[input]]\ntick = 0\nname = \"pick\"\nvalue = \"oak\"\n\
         [[order]]\ntick = 2\ndirective.orders.move.to = [1, 0]\n\
         [[order]]\ntick = 5\ndirective.orders.move.to = [0, 1]\n",
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
    // An input the lane mode declares no input of, or of a list where it takes a string, is
    // refused as the bot starts.
    let refused = |input: &str| {
        let script = OrderScript::parse(&format!("[[input]]\ntick = 0\n{input}\n")).unwrap();
        BotScript::new(&script, &BotNames::new(&books)).unwrap_err()
    };
    let name = |text| DeclaredName::new(text).unwrap();
    assert_eq!(
        refused("name = \"mana\"\nvalue = \"x\""),
        BotScriptError::NoInput(name("mana"))
    );
    assert_eq!(
        refused("name = \"pick\"\nvalue = [\"oak\"]"),
        BotScriptError::InputType {
            name: name("pick"),
            declared: InputType::String,
        }
    );
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
        &bot("directive.orders.slot = { slot = \"first\", target.unit = \"enemies:avatar\" }")
            .unwrap(),
    );
    assert_eq!(cast.slot, Some(ActionName(firsts.clone().into())));
    assert_eq!(
        cast.directive,
        Directive::Orders(OrdersDirective::Slot {
            slot: ActionName(firsts.into()),
            target: ActionTarget::Unit(Filter::resolve(&enemies, tags).unwrap()),
        })
    );
    // A marker's point, the east spawn's at (0, 2), and a point of the map; neither names a slot.
    let walk = |text| first(&bot(text).unwrap());
    let ground = |x, z| GroundPoint::new(Num::int(x), Num::int(z));
    let east = walk("directive.orders.move.to.marker = \"east_spawn\"");
    let point = walk("directive.orders.move.to = [3, -4]");
    for (order, to) in [(&east, ground(0, 2)), (&point, ground(3, -4))] {
        assert_eq!(
            order.directive,
            Directive::Orders(OrdersDirective::Move { to })
        );
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
    let stop = "directive.orders = \"stop\"";
    let named = |of: &str, name: &str| (of.to_owned(), name.to_owned());
    for (text, refused) in [
        (
            format!("units = \"tank\"\n{stop}"),
            named("unit type", "tank"),
        ),
        (
            "directive.progression.learn.slot = \"nope\"".to_owned(),
            named("action", "nope"),
        ),
        (
            "directive.orders.attack.target = \"enemies:flying\"".to_owned(),
            named("filter", "enemies:flying"),
        ),
        (
            "directive.orders.move.to.marker = \"nowhere\"".to_owned(),
            named("marker", "nowhere"),
        ),
        (
            "directive.items.buy = { item = \"potion\", shop = \"allies\" }".to_owned(),
            named("item type", "potion"),
        ),
    ] {
        assert_eq!(unknown(&text), refused, "{text}");
    }
    let refused = |text: &str| bot(text).unwrap_err();
    assert_eq!(
        refused("directive.orders.move.to.marker = \"camp\""),
        BotScriptError::NoPoint(DeclaredName::new("camp").unwrap())
    );
    let past = format!(
        "directive.orders.move.to = [\"{}\", 0]",
        Vec3::BOUND + Num::EPSILON
    );
    assert_eq!(
        refused(&past),
        BotScriptError::Point(MapPointError::OutOfBounds)
    );
    assert_eq!(
        walk("directive.orders.move.to = [21, 0]").directive,
        Directive::Orders(OrdersDirective::Move { to: ground(21, 0) })
    );
    assert_eq!(
        refused("directive.orders.move.to = [1, 0, 1]"),
        BotScriptError::Point(MapPointError::Shape)
    );
    // A search for a place is a build's alone, so a move to one is no script, of a gap above 0,
    // within 128 steps.
    let moved =
        "[[order]]\ntick = 0\ndirective.orders.move.to = { near = [0, 0], gap = 1, steps = 2 }\n";
    assert!(OrderScript::parse(moved).is_err());
    let build = |near: &str| {
        format!(
            "directive.orders.build = {{ slot = \"first\", target.point = {{ at = {near}, angle = 0 }} }}"
        )
    };
    for (text, error) in [
        (
            build("{ near = [0, 0], gap = 0, steps = 2 }"),
            BotScriptError::NearGap(Num::ZERO),
        ),
        (
            build("{ near = [0, 0], gap = 1, steps = 129 }"),
            BotScriptError::NearSteps(129),
        ),
    ] {
        assert_eq!(refused(&text), error, "{text}");
    }
    let near = first(&bot(&build("{ near = [0, 0], gap = 1, steps = 128 }")).unwrap());
    let Directive::Orders(OrdersDirective::Build { target, .. }) = near.directive else {
        panic!("a build");
    };
    let searched = LoadedPlace::Near {
        near: ground(0, 0),
        gap: Num::ONE,
        steps: 128,
    };
    assert_eq!(
        target,
        BuildTarget::Point {
            at: searched,
            angle: Num::ZERO
        }
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

/// What a bot's step gave: the orders of each payload sent, as typed values, so a failure names
/// them, and why each unsent order went unsent.
#[derive(Debug, Default)]
pub(crate) struct Heard {
    pub(crate) sent: Vec<Order>,
    pub(crate) unsent: Vec<OrderUnsent>,
}

impl BotSink for Heard {
    // As the LAN terms let a payload hold.
    fn max_payload_len(&self) -> u32 {
        1024
    }

    fn send(&mut self, _: PlayerSlot, payload: &[u8]) {
        let orders = Order::of_payload(payload).expect("a bot sends orders");
        self.sent.extend(orders);
    }

    fn unsent(&mut self, _: PlayerSlot, _: &Directive<NamedRefs>, cause: OrderUnsent) {
        self.unsent.push(cause);
    }

    fn failed(&mut self, player_slot: PlayerSlot, error: ThinkError) {
        panic!("the think of player slot {player_slot:?} failed: {error}");
    }
}

#[test]
fn a_due_order_finds_its_slot_among_its_units_and_its_target_by_its_filter_or_goes_unsent() {
    // The walker's bot casts its first ability at the nearest enemy hero it sees, in tick 0 and
    // in tick 1. In tick 0 no Vision stage has run, so each hero is seen by its own team alone,
    // as an order may aim only at what its team sees: the cast goes unsent. In tick 1 the
    // walker's team sees the runner, 4 m off, and the cast goes to it, as the walker holds the
    // ability in its slot 0. Then it orders the creeps' attack, which the walker does not hold;
    // attacks a stunned enemy hero, and none is; moves its tower bolts, of which it owns none;
    // and builds by its first ability near the origin, which builds nothing, so finds no place.
    // In tick 2 it attacks the nearest enemy hero, the runner, which its attack selects, and
    // uses none, as the lane holds no unit a hero may use, though it sees the runner. Then the
    // runner's life falls to 0, before the deaths mark it dead: in tick 3 the cast still finds
    // it, as its action aims at nothing and its start takes any unit, but the attack does not,
    // as the attack takes only a living one: each search runs its order's own test.
    let session = FixedSession::new(TestMode::Lane.packages(), 2);
    let mut fixed = session.start();
    let units = MatchUnits::of(fixed.game());
    let [walker, runner] = [0, 1].map(|slot| units.hero(slot));
    let cast = "directive.orders.slot = { slot = \"first\", target.unit = \"enemies:avatar\" }";
    let text = format!(
        "[[order]]\ntick = 0\n{cast}\n[[order]]\ntick = 1\n{cast}\n\
         [[order]]\ntick = 1\ndirective.orders.slot = {{ slot = \"creep_attack\", target = \"none\" }}\n\
         [[order]]\ntick = 1\ndirective.orders.attack.target = \"enemies:avatar:stunned\"\n\
         [[order]]\ntick = 1\nunits = \"tower_bolt\"\ndirective.orders.move.to = [1, 1]\n\
         [[order]]\ntick = 1\ndirective.orders.build = {{ slot = \"first\", target.point = \
         {{ at = {{ near = [0, 0], gap = 1, steps = 2 }}, angle = 0 }} }}\n\
         [[order]]\ntick = 2\ndirective.orders.attack.target = \"enemies:avatar\"\n\
         [[order]]\ntick = 2\ndirective.orders.use.target = \"enemies:avatar\"\n\
         [[order]]\ntick = 3\n{cast}\n\
         [[order]]\ntick = 3\ndirective.orders.attack.target = \"enemies:avatar\"\n"
    );
    let books = session.packages().load_books();
    let names = BotNames::new(books);
    let mut bot = BotScript::new(&OrderScript::parse(&text).unwrap(), &names).unwrap();
    let mut view = HeldBotView::new(fixed.game_mut());
    let slot = PlayerSlot::new(0);
    let mut step = |fixed: &FixedMatch, tick| {
        let mut heard = Heard::default();
        let view = view.get(fixed.game());
        bot.step(
            Tick::new(tick),
            slot,
            &view,
            &mut BotBuffers::default(),
            &mut heard,
        );
        heard
    };
    let first = step(&fixed, 0);
    assert_eq!(first.sent, []);
    assert_eq!(first.unsent, [OrderUnsent::NoTarget]);
    fixed.step();
    let second = step(&fixed, 1);
    let cast = Order::one(
        walker,
        OrdersDirective::Slot {
            slot: 0,
            target: ActionTarget::Unit(runner),
        },
    );
    assert_eq!(second.sent, std::slice::from_ref(&cast));
    assert_eq!(
        second.unsent,
        [
            OrderUnsent::NoSlot,
            OrderUnsent::NoTarget,
            OrderUnsent::NoUnit,
            OrderUnsent::NoPlace
        ]
    );
    fixed.step();
    let third = step(&fixed, 2);
    let attack = Order::one(walker, OrdersDirective::Attack { target: runner });
    assert_eq!(third.sent, [attack]);
    assert_eq!(third.unsent, [OrderUnsent::NoTarget]);
    let runner = fixed.game().unit_entity(runner);
    let mut life = fixed.game_mut().get_mut::<Life>(runner).unwrap();
    let full = life.max();
    life.take(full);
    let fourth = step(&fixed, 3);
    assert_eq!(fourth.sent, [cast]);
    assert_eq!(fourth.unsent, [OrderUnsent::NoTarget]);
}

#[test]
fn a_bot_finds_the_slot_of_the_first_stack_of_an_item_its_unit_carries() {
    // The walker carries a stack of item 1 in its slot 1 and one of item 0 in slot 2: a sale of
    // item 0 takes slot 2, of item 1 slot 1, and of item 2, which it does not carry, none; nor
    // does the runner, which carries no inventory.
    let session = FixedSession::new(TestMode::Lane.packages(), 2);
    let mut fixed = session.start();
    let units = MatchUnits::of(fixed.game());
    let [walker, runner] = [0, 1].map(|slot| units.hero(slot));
    let stack = |item| ItemStack {
        item: ItemId::nth(item),
        count: NonZeroU32::MIN,
        uses: None,
    };
    let mut inventory = Inventory::new(NonZeroU8::new(3).unwrap());
    inventory.restore(1, stack(1));
    inventory.restore(2, stack(0));
    let entity = fixed.game().entity_of(walker).unwrap();
    fixed.game_mut().entity_mut(entity).insert(inventory);
    let mut view = HeldBotView::new(fixed.game_mut());
    let view = view.get(fixed.game());
    let carried = |unit, item| view.carried(unit, ItemId::nth(item));
    assert_eq!(
        [carried(walker, 0), carried(walker, 1), carried(walker, 2)],
        [Some(2), Some(1), None]
    );
    assert_eq!(carried(runner, 0), None);
}
