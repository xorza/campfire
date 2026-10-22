use campfire_actions::ActionTarget;
use campfire_capabilities::{GroundLocation, Location, MarkerLocation, NearLocation};
use campfire_items::ItemsDirective;
use campfire_math::Num;
use campfire_orders::OrdersDirective;
use campfire_progression::ProgressionDirective;
use campfire_units::{BuildTarget, MapPoint};
use campfire_values::FilterData;

use super::*;

/// The order of `directive`, with names, at `tick`, to the avatar.
const fn order(tick: u64, directive: Directive<NamedRefs>) -> ScriptedOrder {
    ScriptedOrder {
        tick: Tick::new(tick),
        units: ScriptedUnits::Avatar,
        directive,
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
        "[[order]]\ntick = 30\ndirective.orders.move.to = [4, 0]\n\n\
         [[order]]\ntick = 30\ndirective.orders.move.to = [\"-2.5\", 1]\n",
    )
    .unwrap();
    let num = |value| Num::from_int(value).unwrap();
    let walk = |to| {
        Directive::Orders(OrdersDirective::Move {
            to: GroundLocation::At(to),
        })
    };
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
            "end = {end}\n[[order]]\ntick = 9\ndirective.orders = \"stop\"\n"
        ))
    };
    assert_eq!(ending(9).unwrap().end(), Some(Tick::new(9)));
    assert_eq!(ending(8), Err(OrderScriptError::EndsEarly { end: 8 }));
    let flaw = |text: &str| OrderScript::parse(text).unwrap_err();
    assert_eq!(
        flaw(
            "[[order]]\ntick = 9\ndirective.orders = \"stop\"\n\
             [[order]]\ntick = 8\ndirective.orders = \"stop\"\n"
        ),
        OrderScriptError::Unordered { tick: 8 }
    );
    // Orders given whole make the script their text reads, which never ends, by the same rule.
    let given = OrderScript::of_orders(script.orders().iter().cloned()).unwrap();
    assert_eq!(given, script);
    let stop = |tick| order(tick, Directive::Orders(OrdersDirective::Stop));
    assert_eq!(
        OrderScript::of_orders([stop(9), stop(8)]),
        Err(OrderScriptError::Unordered { tick: 8 })
    );
    // A coordinate past what a sim number holds, a directive no capability has, two directives,
    // no directive, a filter of no set of relations, and a unit type no mode may declare, fail the
    // read.
    for flawed in [
        "directive.orders.move.to = [99999999999999, 1]",
        "directive.orders.walk.to = [1, 1]",
        "directive.orders = \"stop\"\ndirective.progression.learn.slot = \"mend\"",
        "units = \"ranger\"",
        "directive.orders.attack.target = \"friends\"",
        "units = \"Not A Name\"\ndirective.orders = \"stop\"",
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
        "[[input]]\ntick = 4\nname = \"a\"\nvalue = \"x\"\ndirective.orders = \"stop\"\n",
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
         directive.orders.build = { slot = \"barracks\", target.point = { at = [10, \"-2.5\"], angle = 45 } }\n\
         [[order]]\ntick = 2\nunits = \"ranger\"\ndirective.orders.attack.target = \"enemies:avatar\"\n\
         [[order]]\ntick = 3\ndirective.orders.slot = { slot = \"mend\", target = \"none\" }\n\
         [[order]]\ntick = 3\ndirective.orders.slot = { slot = \"bolt\", target.point = { marker = \"west_spawn\" } }\n\
         [[order]]\ntick = 4\ndirective.progression.learn.slot = \"bolt\"\n\
         [[order]]\ntick = 5\ndirective.items.buy = { item = \"potion\", shop = \"allies:shop\" }\n\
         [[order]]\ntick = 6\ndirective.orders.move.to.marker = \"east_spawn\"\n\
         [[order]]\ntick = 7\nunits = \"dozer\"\ndirective.orders.build = { slot = \"barracks\", target.point = \
         { at = { near = { marker = \"west_spawn\" }, gap = \"2.5\", steps = 8 }, angle = 0 } }\n\
         [[order]]\ntick = 8\ndirective.orders.attack_move.to = [10, \"-2.5\"]\n",
    )
    .unwrap();
    let num = |value| Num::from_int(value).unwrap();
    let filter = |text| FilterData::parse(text).unwrap();
    let marker = |text| Location::Marker(MarkerLocation { marker: name(text) });
    let typed = |text| ScriptedUnits::Typed(name(text));
    let build = OrdersDirective::Build {
        slot: name("barracks"),
        target: BuildTarget::Point {
            at: GroundLocation::At(at(num(10), num(-3) + Num::HALF)),
            angle: num(45),
        },
    };
    let near = OrdersDirective::Build {
        slot: name("barracks"),
        target: BuildTarget::Point {
            at: GroundLocation::Near(NearLocation {
                near: marker("west_spawn"),
                gap: num(2) + Num::HALF,
                steps: 8,
            }),
            angle: Num::ZERO,
        },
    };
    let attack = OrdersDirective::Attack {
        target: filter("enemies:avatar"),
    };
    let expected = [
        ScriptedOrder {
            units: typed("dozer"),
            ..order(1, Directive::Orders(build))
        },
        ScriptedOrder {
            units: typed("ranger"),
            ..order(2, Directive::Orders(attack))
        },
        order(
            3,
            Directive::Orders(OrdersDirective::Slot {
                slot: name("mend"),
                target: ActionTarget::None,
            }),
        ),
        order(
            3,
            Directive::Orders(OrdersDirective::Slot {
                slot: name("bolt"),
                target: ActionTarget::Point(marker("west_spawn")),
            }),
        ),
        order(
            4,
            Directive::Progression(ProgressionDirective::Learn { slot: name("bolt") }),
        ),
        order(
            5,
            Directive::Items(ItemsDirective::Buy {
                item: name("potion"),
                shop: filter("allies:shop"),
            }),
        ),
        order(
            6,
            Directive::Orders(OrdersDirective::Move {
                to: GroundLocation::At(marker("east_spawn")),
            }),
        ),
        ScriptedOrder {
            units: typed("dozer"),
            ..order(7, Directive::Orders(near))
        },
        order(
            8,
            Directive::Orders(OrdersDirective::AttackMove {
                to: GroundLocation::At(at(num(10), num(-3) + Num::HALF)),
            }),
        ),
    ];
    assert_eq!(script.orders(), expected);
}
