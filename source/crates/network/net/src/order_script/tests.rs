use bevy_ecs::system::SystemState;
use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, IdAllocator};
use campfire_units::{Avatar, Dead, Owner};

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
                units: ScriptedUnits::Avatar,
                action: Action::Orders(OrdersAction::Move {
                    x: num(4),
                    z: Num::ZERO
                }),
            },
            ScriptedOrder {
                tick: Tick::new(30),
                units: ScriptedUnits::Avatar,
                action: Action::Orders(OrdersAction::Move {
                    x: num(-3) + half,
                    z: num(1)
                }),
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
    // A coordinate past what a sim number holds, and an unknown action, fail the read.
    for flawed in ["move = [99999999999999, 1]", "walk = [1, 1]"] {
        let text = format!("[[order]]\ntick = 9\n{flawed}\n");
        assert!(matches!(flaw(&text), OrderScriptError::Toml(_)), "{flawed}");
    }
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
        (
            Tick::new(10),
            Action::Progression(ProgressionAction::Learn { slot: 1 })
        )
    );
    assert_eq!(
        (first.tick, first.action),
        (
            Tick::new(9),
            Action::Orders(OrdersAction::Slot {
                slot: 2,
                target: ActionTarget::None
            })
        )
    );
    assert!(matches!(
        (second.tick.get(), second.action),
        (9, Action::Orders(OrdersAction::Slot { slot: 0, target: ActionTarget::Unit(unit) })) if unit.get() == 7
    ));
    assert!(matches!(
        (third.tick.get(), third.action),
        (10, Action::Orders(OrdersAction::Attack { target })) if target.get() == 3
    ));
    let (none, many, stray) = (
        OrderScriptError::NoAction { tick: 9 },
        OrderScriptError::ManyActions { tick: 9 },
        OrderScriptError::StrayTarget { tick: 9 },
    );
    for (flawed, error) in [
        ("[[order]]\ntick = 9\n", none.clone()),
        ("[[order]]\ntick = 9\ntarget = 3\n", none),
        (
            "[[order]]\ntick = 9\ncast = 0\nmove = [1, 1]\n",
            many.clone(),
        ),
        ("[[order]]\ntick = 9\nattack = 3\ncast = 0\n", many.clone()),
        ("[[order]]\ntick = 9\nlearn = 0\ncast = 0\n", many),
        (
            "[[order]]\ntick = 9\nattack = 3\ntarget = 4\n",
            stray.clone(),
        ),
        ("[[order]]\ntick = 9\nlearn = 0\ntarget = 4\n", stray),
    ] {
        assert_eq!(flaw(flawed), error, "{flawed}");
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

#[test]
fn an_order_names_its_units_by_type_and_a_build_its_place() {
    let script = OrderScript::parse(
        "[[order]]\ntick = 1\nunits = \"dozer\"\nbuild = { slot = 1, x = 10, z = \"-2.5\", angle = 45 }\n\
         [[order]]\ntick = 2\nunits = \"ranger\"\nattack = 7\n",
    )
    .unwrap();
    let typed = |name| ScriptedUnits::Typed(DeclaredName::new(name).unwrap());
    let num = |value| Num::from_int(value).unwrap();
    let [build, attack] = script.orders() else {
        panic!("two orders");
    };
    assert_eq!(
        *build,
        ScriptedOrder {
            tick: Tick::new(1),
            units: typed("dozer"),
            action: Action::Orders(OrdersAction::Build {
                slot: 1,
                target: BuildTarget::Point {
                    x: num(10),
                    z: num(-3) + Num::HALF,
                    angle: num(45),
                },
            }),
        }
    );
    assert_eq!(attack.units, typed("ranger"));
    assert!(matches!(
        attack.action,
        Action::Orders(OrdersAction::Attack { target }) if target.get() == 7
    ));
    // A build with another action is two; a build missing its angle, a target with a build,
    // and a unit type no mode may declare, are refused.
    let flaw = |text: &str| OrderScript::parse(text).unwrap_err();
    let build = "build = { slot = 1, x = 1, z = 1, angle = 0 }";
    assert_eq!(
        flaw(&format!("[[order]]\ntick = 9\n{build}\nmove = [1, 1]\n")),
        OrderScriptError::ManyActions { tick: 9 }
    );
    assert_eq!(
        flaw(&format!("[[order]]\ntick = 9\n{build}\ntarget = 3\n")),
        OrderScriptError::StrayTarget { tick: 9 }
    );
    for flawed in [
        "[[order]]\ntick = 9\nbuild = { slot = 1, x = 1, z = 1 }\n",
        "[[order]]\ntick = 9\nunits = \"Not A Name\"\nmove = [1, 1]\n",
    ] {
        assert!(
            matches!(flaw(flawed), OrderScriptError::Toml(_)),
            "{flawed}"
        );
    }
}

#[test]
fn an_order_commands_the_avatar_or_the_living_units_of_its_type_or_none() {
    let mut world = World::new();
    world.init_resource::<EntityIndex>();
    let mut ids = IdAllocator::default();
    let [bot, other] = [1, 0].map(|slot| Owner::new(PlayerSlot::new(slot)));
    let types: Names<UnitType> = [(TypeScope::Mode, "dozer"), (TypeScope::Mode, "ranger")].into();
    let [dozer, ranger] = [UnitType::new(0), UnitType::new(1)];
    // The bot's rangers 0, 2 and 4, but 2 is dead; a ranger of the other player, and the bot's
    // dozer.
    let mut spawned = Vec::new();
    for (owner, unit_type) in [
        (bot, ranger),
        (other, ranger),
        (bot, ranger),
        (bot, dozer),
        (bot, ranger),
    ] {
        let id = ids.allocate();
        let entity = world.spawn((id, owner, unit_type)).id();
        spawned.push((id, entity));
    }
    world.entity_mut(spawned[2].1).insert(Dead);
    let mut state: SystemState<PlayerUnits<'static, 'static>> = SystemState::new(&mut world);
    let order = |text: &str, world: &World, state: &mut SystemState<_>| {
        let script = OrderScript::parse(&format!("[[order]]\ntick = 0\n{text}move = [1, 1]\n"));
        let players = state.get(world).unwrap();
        let mut scratch = Vec::new();
        script.unwrap().orders()[0].order(PlayerSlot::new(1), &players, &types, &mut scratch)
    };
    let rangers = order("units = \"ranger\"\n", &world, &mut state).unwrap();
    assert_eq!(rangers.units.get(), [spawned[0].0, spawned[4].0]);
    let dozers = order("units = \"dozer\"\n", &world, &mut state).unwrap();
    assert_eq!(dozers.units.get(), [spawned[3].0]);
    // A type the match lacks, a type the bot owns none of alive, and an avatar it lacks.
    world.entity_mut(spawned[3].1).insert(Dead);
    let missing = |text: &str, world: &World, state: &mut SystemState<_>| {
        order(text, world, state).unwrap_err()
    };
    assert_eq!(
        missing("units = \"tank\"\n", &world, &mut state),
        UnitsMissing::UnknownType
    );
    assert_eq!(
        missing("units = \"dozer\"\n", &world, &mut state),
        UnitsMissing::NoUnit
    );
    assert_eq!(missing("", &world, &mut state), UnitsMissing::NoAvatar);
    // An avatar of the bot takes the order alone.
    world.entity_mut(spawned[4].1).insert(Avatar);
    let avatar = order("", &world, &mut state).unwrap();
    assert_eq!(avatar.units.get(), [spawned[4].0]);
}
