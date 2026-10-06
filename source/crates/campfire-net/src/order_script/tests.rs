use campfire_math::Num;

use super::*;

#[test]
fn a_script_reads_its_orders_in_tick_order_or_its_flaw() {
    let script = OrderScript::parse(
        "[[order]]\ntick = 30\nmove = [4, 0]\n\n[[order]]\ntick = 30\nmove = [\"-2.5\", 1]\n",
    )
    .unwrap();
    let half = Num::HALF;
    let num = |value| Num::from_int(value).unwrap();
    assert_eq!(
        script.orders(),
        [
            ScriptedOrder {
                tick: Tick::new(30),
                action: Action::Move {
                    x: num(4),
                    z: Num::ZERO
                },
            },
            ScriptedOrder {
                tick: Tick::new(30),
                action: Action::Move {
                    x: num(-3) + half,
                    z: num(1)
                },
            },
        ]
    );
    assert_eq!(script.end(), None);
    let empty = OrderScript::parse("").unwrap();
    assert_eq!((empty.orders(), empty.end()), (&[][..], None));
    // The script may end in the tick of its last order, not before.
    let ending = |end: u64| {
        OrderScript::parse(&format!(
            "end = {end}\n[[order]]\ntick = 9\nmove = [1, 1]\n"
        ))
    };
    assert_eq!(ending(9).unwrap().end(), Some(Tick::new(9)));
    assert_eq!(ending(8), Err(OrderScriptError::EndsEarly { end: 8 }));
    let flaw = |text: &str| OrderScript::parse(text).unwrap_err();
    assert_eq!(
        flaw("[[order]]\ntick = 9\nmove = [1, 1]\n[[order]]\ntick = 8\nmove = [1, 1]\n"),
        OrderScriptError::Unordered { tick: 8 }
    );
    assert_eq!(
        flaw("[[order]]\ntick = 9\nmove = [99999999999999, 1]\n"),
        OrderScriptError::Coordinate { tick: 9 }
    );
    assert!(matches!(
        flaw("[[order]]\ntick = 9\nwalk = [1, 1]\n"),
        OrderScriptError::Toml(_)
    ));
    // A cast of slot 2 with no target, one of slot 0 at unit 7, an attack on unit 3, and a learn
    // of slot 1; an order of no action, of two, or of a target with no cast is refused.
    let actions = "[[order]]\ntick = 9\ncast = 2\n[[order]]\ntick = 9\ncast = 0\ntarget = 7\n\
                       [[order]]\ntick = 10\nattack = 3\n[[order]]\ntick = 10\nlearn = 1\n";
    let orders = OrderScript::parse(actions).unwrap();
    let [first, second, third, fourth] = orders.orders() else {
        panic!("four orders");
    };
    assert_eq!(
        (fourth.tick, fourth.action),
        (Tick::new(10), Action::Learn { slot: 1 })
    );
    assert_eq!(
        (first.tick, first.action),
        (
            Tick::new(9),
            Action::Slot {
                slot: 2,
                target: ActionTarget::None
            }
        )
    );
    assert!(matches!(
        (second.tick.get(), second.action),
        (9, Action::Slot { slot: 0, target: ActionTarget::Unit(unit) }) if unit.get() == 7
    ));
    assert!(matches!(
        (third.tick.get(), third.action),
        (10, Action::Attack { target }) if target.get() == 3
    ));
    for flawed in [
        "[[order]]\ntick = 9\n",
        "[[order]]\ntick = 9\ncast = 0\nmove = [1, 1]\n",
        "[[order]]\ntick = 9\nattack = 3\ncast = 0\n",
        "[[order]]\ntick = 9\ntarget = 3\n",
        "[[order]]\ntick = 9\nattack = 3\ntarget = 4\n",
        "[[order]]\ntick = 9\nlearn = 0\ncast = 0\n",
        "[[order]]\ntick = 9\nlearn = 0\ntarget = 4\n",
    ] {
        assert_eq!(
            flaw(flawed),
            OrderScriptError::Action { tick: 9 },
            "{flawed}"
        );
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
        "[[input]]\ntick = 4\nname = \"a\"\nvalue = \"x\"\nmove = [1, 1]\n",
        "[[input]]\ntick = 4\nvalue = \"x\"\n",
    ] {
        assert!(
            matches!(flaw(flawed), OrderScriptError::Toml(_)),
            "{flawed}"
        );
    }
}
