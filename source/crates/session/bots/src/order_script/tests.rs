use campfire_actions::ActionTarget;
use campfire_items::ItemsAction;
use campfire_math::Num;
use campfire_orders::OrdersAction;
use campfire_progression::ProgressionAction;
use campfire_units::{BuildTarget, MapPoint};
use campfire_values::FilterData;

use super::*;
use crate::location::{Location, MarkerLocation};

/// The order of `action`, with names, at `tick`, to the avatar.
const fn order(tick: u64, action: Action<Named>) -> ScriptedOrder {
    ScriptedOrder {
        tick: Tick::new(tick),
        units: ScriptedUnits::Avatar,
        action,
    }
}

/// The name `name`, which a test writes right.
fn name(name: &str) -> DeclaredName {
    DeclaredName::new(name).unwrap()
}

/// The location on the ground at `x`, `z`.
fn at(x: Num, z: Num) -> Location {
    Location::At(MapPoint::Ground([x, z]))
}

#[test]
fn a_script_reads_its_orders_in_tick_order_or_its_flaw() {
    let script = OrderScript::parse(
        "[[order]]\ntick = 30\naction.orders.move.to = [4, 0]\n\n\
         [[order]]\ntick = 30\naction.orders.move.to = [\"-2.5\", 1]\n",
    )
    .unwrap();
    let num = |value| Num::from_int(value).unwrap();
    let walk = |to| Action::Orders(OrdersAction::Move { to });
    assert_eq!(
        script.orders(),
        [
            order(30, walk(at(num(4), Num::ZERO))),
            order(30, walk(at(num(-3) + Num::HALF, num(1)))),
        ]
    );
    assert_eq!(script.end(), None);
    let empty = OrderScript::parse("").unwrap();
    assert_eq!((empty.orders(), empty.end()), (&[][..], None));
    // The script may end in the tick of its last order, not before.
    let ending = |end: u64| {
        OrderScript::parse(&format!(
            "end = {end}\n[[order]]\ntick = 9\naction.orders = \"stop\"\n"
        ))
    };
    assert_eq!(ending(9).unwrap().end(), Some(Tick::new(9)));
    assert_eq!(ending(8), Err(OrderScriptError::EndsEarly { end: 8 }));
    let flaw = |text: &str| OrderScript::parse(text).unwrap_err();
    assert_eq!(
        flaw(
            "[[order]]\ntick = 9\naction.orders = \"stop\"\n\
             [[order]]\ntick = 8\naction.orders = \"stop\"\n"
        ),
        OrderScriptError::Unordered { tick: 8 }
    );
    // A coordinate past what a sim number holds, an action no capability has, two actions, no
    // action, a filter of no set of relations, and a unit type no mode may declare, fail the
    // read.
    for flawed in [
        "action.orders.move.to = [99999999999999, 1]",
        "action.orders.walk.to = [1, 1]",
        "action.orders = \"stop\"\naction.progression.learn.slot = \"mend\"",
        "units = \"ranger\"",
        "action.orders.attack.target = \"friends\"",
        "units = \"Not A Name\"\naction.orders = \"stop\"",
    ] {
        let text = format!("[[order]]\ntick = 9\n{flawed}\n");
        assert!(matches!(flaw(&text), OrderScriptError::Toml(_)), "{flawed}");
    }
}

#[test]
fn a_script_reads_its_mode_inputs_in_tick_order_or_its_flaw() {
    let script = OrderScript::parse(
        "end = 20\n[[input]]\ntick = 0\nname = \"hero\"\nvalue = \"hero-x\"\n\
         [[input]]\ntick = 0\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    )
    .unwrap();
    assert_eq!(
        script.inputs(),
        [
            ScriptedInput {
                tick: Tick::new(0),
                name: "hero".to_owned(),
                value: ScriptedValue::Text("hero-x".to_owned()),
            },
            ScriptedInput {
                tick: Tick::new(0),
                name: "spells".to_owned(),
                value: ScriptedValue::List(vec!["haste".to_owned(), "mend".to_owned()]),
            },
        ]
    );
    assert_eq!(script.orders(), []);
    let flaw = |text: &str| OrderScript::parse(text).unwrap_err();
    assert_eq!(
        flaw(
            "[[input]]\ntick = 5\nname = \"a\"\nvalue = \"x\"\n\
              [[input]]\ntick = 4\nname = \"a\"\nvalue = \"x\"\n"
        ),
        OrderScriptError::Unordered { tick: 4 }
    );
    assert_eq!(
        flaw("end = 3\n[[input]]\ntick = 4\nname = \"a\"\nvalue = \"x\"\n"),
        OrderScriptError::EndsEarly { end: 3 }
    );
    // A value of another shape, and a field the entry does not know.
    for flawed in [
        "[[input]]\ntick = 4\nname = \"a\"\nvalue = 3\n",
        "[[input]]\ntick = 4\nname = \"a\"\nvalue = \"x\"\naction.orders = \"stop\"\n",
        "[[input]]\ntick = 4\nvalue = \"x\"\n",
    ] {
        assert!(
            matches!(flaw(flawed), OrderScriptError::Toml(_)),
            "{flawed}"
        );
    }
}

#[test]
fn an_order_names_its_units_by_type_its_slot_and_item_by_name_its_target_by_a_filter() {
    let script = OrderScript::parse(
        "[[order]]\ntick = 1\nunits = \"dozer\"\n\
         action.orders.build = { slot = \"barracks\", target.point = { at = [10, \"-2.5\"], angle = 45 } }\n\
         [[order]]\ntick = 2\nunits = \"ranger\"\naction.orders.attack.target = \"enemies:avatar\"\n\
         [[order]]\ntick = 3\naction.orders.slot = { slot = \"mend\", target = \"none\" }\n\
         [[order]]\ntick = 3\naction.orders.slot = { slot = \"bolt\", target.point = { marker = \"west_spawn\" } }\n\
         [[order]]\ntick = 4\naction.progression.learn.slot = \"bolt\"\n\
         [[order]]\ntick = 5\naction.items.buy = { item = \"potion\", shop = \"allies:shop\" }\n\
         [[order]]\ntick = 6\naction.orders.move.to.marker = \"east_spawn\"\n",
    )
    .unwrap();
    let num = |value| Num::from_int(value).unwrap();
    let filter = |text| FilterData::parse(text).unwrap();
    let marker = |text| Location::Marker(MarkerLocation { marker: name(text) });
    let typed = |text| ScriptedUnits::Typed(name(text));
    let build = OrdersAction::Build {
        slot: name("barracks"),
        target: BuildTarget::Point {
            at: at(num(10), num(-3) + Num::HALF),
            angle: num(45),
        },
    };
    let attack = OrdersAction::Attack {
        target: filter("enemies:avatar"),
    };
    let expected = [
        ScriptedOrder {
            units: typed("dozer"),
            ..order(1, Action::Orders(build))
        },
        ScriptedOrder {
            units: typed("ranger"),
            ..order(2, Action::Orders(attack))
        },
        order(
            3,
            Action::Orders(OrdersAction::Slot {
                slot: name("mend"),
                target: ActionTarget::None,
            }),
        ),
        order(
            3,
            Action::Orders(OrdersAction::Slot {
                slot: name("bolt"),
                target: ActionTarget::Point(marker("west_spawn")),
            }),
        ),
        order(
            4,
            Action::Progression(ProgressionAction::Learn { slot: name("bolt") }),
        ),
        order(
            5,
            Action::Items(ItemsAction::Buy {
                item: name("potion"),
                shop: filter("allies:shop"),
            }),
        ),
        order(
            6,
            Action::Orders(OrdersAction::Move {
                to: marker("east_spawn"),
            }),
        ),
    ];
    assert_eq!(script.orders(), expected);
}
