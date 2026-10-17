use bevy_ecs::system::SystemState;
use bevy_ecs::world::World;
use campfire_common::DeclaredName;
use campfire_sim::{EntityIndex, IdAllocator};
use campfire_units::{Avatar, Dead, Owner};

use super::*;

#[test]
fn a_bot_takes_its_inputs_and_its_orders_apart_each_once() {
    let script = OrderScript::parse(
        "[[input]]\ntick = 0\nname = \"hero\"\nvalue = \"hero-x\"\n\
         [[order]]\ntick = 2\nmove = [1, 0]\n[[order]]\ntick = 5\nmove = [0, 1]\n",
    )
    .unwrap();
    let ticks =
        |orders: &[BotOrder]| -> Vec<u64> { orders.iter().map(|order| order.tick.get()).collect() };
    let mut bot = BotScript::new(&script, &Names::default()).unwrap();
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
    let script = |text: &str| {
        let text = format!("[[order]]\ntick = 0\n{text}move = [1, 1]\n");
        BotScript::new(&OrderScript::parse(&text).unwrap(), &types)
    };
    let mut state: SystemState<PlayerUnits<'static, 'static>> = SystemState::new(&mut world);
    let order = |text: &str, world: &World, state: &mut SystemState<_>| {
        let mut bot = script(text).unwrap();
        let players = state.get(world).unwrap();
        let mut scratch = Vec::new();
        bot.due_orders(Tick::new(0))[0].order(PlayerSlot::new(1), &players, &mut scratch)
    };
    let rangers = order("units = \"ranger\"\n", &world, &mut state).unwrap();
    assert_eq!(rangers.units.get(), [spawned[0].0, spawned[4].0]);
    let dozers = order("units = \"dozer\"\n", &world, &mut state).unwrap();
    assert_eq!(dozers.units.get(), [spawned[3].0]);
    // A type the match lacks fails the script as it starts; a type the bot owns none of alive,
    // and an avatar it lacks, fail the order as it is due.
    let tank = DeclaredName::new("tank").unwrap();
    assert_eq!(
        script("units = \"tank\"\n").unwrap_err(),
        UnknownUnitType { name: tank }
    );
    world.entity_mut(spawned[3].1).insert(Dead);
    let missing = |text: &str, world: &World, state: &mut SystemState<_>| {
        order(text, world, state).unwrap_err()
    };
    assert_eq!(
        missing("units = \"dozer\"\n", &world, &mut state),
        UnitsMissing::NoUnit
    );
    assert_eq!(missing("", &world, &mut state), UnitsMissing::NoAvatar);
    // An avatar of the bot takes the order alone.
    world.entity_mut(spawned[4].1).insert(Avatar);
    let avatar = order("", &world, &mut state).unwrap();
    assert_eq!(avatar.units.get(), [spawned[4].0]);

    // A step sends the bot's mode inputs, then its orders, each a payload as a client writes it,
    // and tells of an order of no living unit, which goes unsent: the dozer is dead.
    let text = "[[input]]\ntick = 0\nname = \"hero\"\nvalue = \"hero-x\"\n\
                [[order]]\ntick = 0\nunits = \"ranger\"\nmove = [1, 1]\n\
                [[order]]\ntick = 0\nunits = \"dozer\"\nmove = [1, 1]\n";
    let mut bot = BotScript::new(&OrderScript::parse(text).unwrap(), &types).unwrap();
    let mut apart = bot.clone();
    let players = state.get(&world).unwrap();
    let mut heard = Heard::default();
    let slot = PlayerSlot::new(1);
    bot.step(
        Tick::new(0),
        slot,
        &players,
        &mut BotBuffers::default(),
        &mut heard,
    );
    let (mut body, mut input, mut ordered) = (Vec::new(), Vec::new(), Vec::new());
    apart.due_inputs(Tick::new(0))[0].write_payload(&mut body, &mut input);
    let rangers = apart.due_orders(Tick::new(0))[0].order(slot, &players, &mut Vec::new());
    rangers.unwrap().write_payload(&mut body, &mut ordered);
    assert_eq!(heard.sent, [input, ordered]);
    assert_eq!(heard.unordered, [(slot, UnitsMissing::NoUnit)]);
}

/// What a step gave: each payload sent, and each order of no unit.
#[derive(Debug, Default)]
struct Heard {
    sent: Vec<Vec<u8>>,
    unordered: Vec<(PlayerSlot, UnitsMissing)>,
}

impl BotSink for Heard {
    fn send(&mut self, _: PlayerSlot, payload: &[u8]) {
        self.sent.push(payload.to_vec());
    }

    fn unordered(&mut self, slot: PlayerSlot, _: Action, cause: UnitsMissing) {
        self.unordered.push((slot, cause));
    }
}
