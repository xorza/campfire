use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use campfire_math::{Num, SegmentSeed};
use campfire_sim::{Capability, IdAllocator, SimTick, SimUpdate, StableId, TickInput, TypeHash};

use super::*;
use crate::combat::Combat;
use crate::combat::combatant::Combatant;
use crate::combat::health::Health;
use crate::combat::on_death::OnDeath;
use crate::navigation::Navigation;
use crate::navigation::lane_walker::PathDirection;
use crate::navigation::move_step::MoveStep;

const ONE: i64 = 1 << 24;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn at(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(num(x), num(y), num(z))).unwrap()
}

fn raw(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(
        Num::from_bits(x),
        Num::from_bits(y),
        Num::from_bits(z),
    ))
    .unwrap()
}

/// `health`, and `damage` within `range`, a windup and a period in ticks; `Stay` on death, as a
/// hero.
fn combatant(health: i64, range: i64, windup: u32, period: u32, damage: i64) -> Combatant {
    Combatant {
        health: Health::new(num(health)).unwrap(),
        attack: AttackStats::new(num(range), windup, period, num(damage)).unwrap(),
        on_death: OnDeath::Stay,
    }
}

/// 100 health, and 30 damage within 2 m, 2 ticks after the start of an attack every 5 ticks.
fn fighter_stats() -> Combatant {
    combatant(100, 2, 2, 5, 30)
}

/// A still target that never attacks and despawns when it dies.
fn dummy(health: i64) -> Combatant {
    Combatant {
        on_death: OnDeath::Despawn,
        ..combatant(health, 0, 0, 1, 0)
    }
}

fn meter() -> MoveStep {
    MoveStep::new(Num::ONE).unwrap()
}

/// A match with all three capabilities.
#[derive(Debug)]
struct Match {
    world: World,
    registry: StateRegistry,
}

impl Match {
    fn new() -> Match {
        Match::with_lanes(Lanes::default())
    }

    fn with_lanes(lanes: Lanes) -> Match {
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]));
        let mut schedule = SimUpdate::schedule();
        let mut registry = StateRegistry::new();
        Combat::install(&mut world, &mut schedule, &mut registry);
        Navigation::install(&mut world, &mut schedule, &mut registry);
        Control::install(&mut schedule, &mut registry);
        world.insert_resource(lanes);
        world.add_schedule(schedule);
        Match { world, registry }
    }

    fn spawn(&mut self, at: Position, parts: impl Bundle) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        self.world.spawn((id, at, parts));
        id
    }

    /// A hero of `slot` that walks a meter a tick.
    fn hero(&mut self, slot: u32, team: Team, at: Position, combatant: Combatant) -> StableId {
        self.spawn(
            at,
            (
                combatant.bundle(team),
                meter().bundle(),
                Controller::new(slot),
            ),
        )
    }

    /// A still unit, and a tower when `tower`.
    fn still(&mut self, team: Team, at: Position, combatant: Combatant, tower: bool) -> StableId {
        let id = self.spawn(at, combatant.bundle(team));
        if tower {
            let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
            self.world.entity_mut(entity).insert(TowerAi);
        }
        id
    }

    fn tick(&mut self, inputs: &[(u32, &[u8])]) {
        let mut tick_inputs = self.world.resource_mut::<TickInputs>();
        for &(slot, payload) in inputs {
            tick_inputs.push(TickInput { slot, payload });
        }
        self.world.run_schedule(SimUpdate);
    }

    /// Runs ticks with no inputs until `tick` is the next.
    fn run_until(&mut self, tick: u64) {
        while self.world.resource::<SimTick>().get() < tick {
            self.tick(&[]);
        }
    }

    fn get<C: Component + Copy>(&self, id: StableId) -> Option<C> {
        let entity = self.world.resource::<EntityIndex>().get(id)?;
        self.world.entity(entity).get::<C>().copied()
    }

    fn position(&self, id: StableId) -> Position {
        self.get::<Position>(id).unwrap()
    }

    fn destination(&self, id: StableId) -> Option<Position> {
        self.get::<Destination>(id).unwrap().get()
    }

    /// `None` once the unit despawned.
    fn health(&self, id: StableId) -> Option<i64> {
        self.get::<Health>(id)
            .map(|health| health.current().round())
    }

    fn attack(&self, id: StableId) -> AttackState {
        self.get::<AttackState>(id).unwrap()
    }

    fn dead(&self, id: StableId) -> bool {
        self.get::<Dead>(id).is_some()
    }
}

fn move_to(unit: StableId, x: i64, z: i64) -> Vec<u8> {
    Order::payload(&[Order {
        unit,
        action: Action::Move {
            x: num(x),
            z: num(z),
        },
    }])
}

fn attack(unit: StableId, target: StableId) -> Vec<u8> {
    Order::payload(&[Order {
        unit,
        action: Action::Attack { target },
    }])
}

fn id(value: u8) -> StableId {
    postcard::from_bytes(&[value]).unwrap()
}

#[test]
fn orders_decode_exactly() {
    let order = Order {
        unit: id(4),
        action: Action::Move {
            x: num(-3),
            z: Num::from_bits(5),
        },
    };
    let body = order.encode();
    assert_eq!(Order::decode(&body), Some(order));
    assert_eq!(Order::decode(&[body.as_slice(), &[0]].concat()), None);
    assert_eq!(Order::decode(&body[..body.len() - 1]), None);
    assert_eq!(Order::decode(&[]), None);

    // The unit's id, then variant 1 with the target's id, each a varint: 300 = 0xAC 0x02.
    let target = postcard::from_bytes::<StableId>(&[0xAC, 0x02]).unwrap();
    let attack = Order {
        unit: target,
        action: Action::Attack { target },
    };
    assert_eq!(attack.encode(), [0xAC, 0x02, 1, 0xAC, 0x02]);
    assert_eq!(Order::decode(&[0xAC, 0x02, 1, 0xAC, 0x02]), Some(attack));
    // Variant 2 does not exist.
    assert_eq!(Order::decode(&[0, 2, 0]), None);

    // A payload is a list of `orders` commands, one per order.
    let payload = Order::payload(&[order, attack]);
    let commands: Vec<_> = Command::decode(&payload).unwrap().collect();
    assert_eq!(commands.len(), 2);
    assert!(
        commands
            .iter()
            .all(|command| command.capability == Capability::Orders)
    );
    assert_eq!(commands[1].body, attack.encode());
}

/// Heroes of slots 0 and 1, a meter a tick: slot 0 at the origin, slot 1 at x = 4 and y = 2.
fn two_heroes() -> (Match, [StableId; 2]) {
    let mut game = Match::new();
    let heroes = [
        game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats()),
        game.hero(1, Team::new(0), at(4, 2, 0), fighter_stats()),
    ];
    (game, heroes)
}

#[test]
fn a_hero_walks_to_its_players_target() {
    let (mut game, [first, second]) = two_heroes();
    game.tick(&[(0, &move_to(first, 0, 5))]);
    // Straight along z, one meter a tick: exact, and done in the fifth tick.
    assert_eq!(game.position(first), at(0, 0, 1));
    assert_eq!(game.destination(first), Some(at(0, 0, 5)));
    assert_eq!(game.position(second), at(4, 2, 0));
    assert_eq!(game.destination(second), None);
    game.run_until(5);
    assert_eq!(game.position(first), at(0, 0, 5));
    assert_eq!(game.destination(first), None);

    // Offset (3, 0, 4) from slot 1's hero, distance 5: the first meter is 3/5 and 4/5, each
    // rounded once: 10 066 329.6 → 10 066 330 and 13 421 772.8 → 13 421 773. The order keeps
    // the hero's height, y = 2.
    game.tick(&[(1, &move_to(second, 7, 4))]);
    assert_eq!(
        game.position(second),
        raw(4 * ONE + 10_066_330, 2 * ONE, 13_421_773)
    );
    assert_eq!(game.destination(second), Some(at(7, 2, 4)));
}

#[test]
fn only_its_players_orders_in_the_orders_capability_move_a_unit() {
    let (mut game, [first, second]) = two_heroes();
    game.tick(&[(0, &move_to(first, 0, 5)), (0, &move_to(first, 0, -5))]);
    // The last order in a tick wins.
    assert_eq!(game.position(first), at(0, 0, -1));
    assert_eq!(game.destination(first), Some(at(0, 0, -5)));

    let beyond = Order::payload(&[Order {
        unit: first,
        action: Action::Move {
            x: Position::BOUND + Num::EPSILON,
            z: Num::ZERO,
        },
    }]);
    let edge = Order::payload(&[Order {
        unit: second,
        action: Action::Move {
            x: Position::BOUND,
            z: Num::ZERO,
        },
    }]);
    let not_an_order = Command::encode(&[Command {
        capability: Order::CAPABILITY,
        body: b"not an order",
    }]);
    // A valid order, sent to a capability the mode did not declare.
    let order = Order {
        unit: first,
        action: Action::Move {
            x: num(9),
            z: num(9),
        },
    }
    .encode();
    let undeclared = Command::encode(&[Command {
        capability: Capability::Character,
        body: &order,
    }]);
    game.tick(&[
        (0, &beyond),
        (0, &not_an_order),
        (0, &undeclared),
        (0, &order),
        (0, &move_to(second, 9, 9)),
        (2, &move_to(first, 9, 9)),
        (1, &edge),
    ]);
    // None reaches the first hero, which walks on; the second takes only its own player's.
    assert_eq!(game.position(first), at(0, 0, -2));
    assert_eq!(game.destination(first), Some(at(0, 0, -5)));
    assert_eq!(
        game.destination(second),
        Some(Position::new(Vec3::new(Position::BOUND, num(2), Num::ZERO)).unwrap())
    );
}

#[test]
fn an_attack_chases_winds_up_and_strikes_each_period() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    let dummy = game.still(Team::new(1), at(6, 0, 0), dummy(100), false);

    // Ticks 0 to 3 walk 4 m, to 2 m from the dummy. Tick 4 starts an attack: it strikes in tick
    // 6, and the next may start in tick 4 + 5 = 9.
    game.tick(&[(0, &attack(fighter, dummy))]);
    game.run_until(4);
    assert_eq!(game.position(fighter), at(4, 0, 0));
    assert_eq!(game.attack(fighter).started(), None);
    game.run_until(6);
    assert_eq!(game.attack(fighter).started(), Some(4));
    assert_eq!(game.health(dummy), Some(100));
    game.run_until(7);
    assert_eq!(game.health(dummy), Some(70));
    assert_eq!(game.attack(fighter).ready_at(), 9);
    assert_eq!(game.destination(fighter), None);

    // Strikes in ticks 11, 16 and 21 take 70 to 40, 10 and 0: the dummy despawns in tick 21.
    game.run_until(21);
    assert_eq!(game.health(dummy), Some(10));
    game.run_until(22);
    assert_eq!(game.health(dummy), None);
    // The next tick finds the target gone, drops it, and stays.
    game.run_until(23);
    assert_eq!(game.attack(fighter).target(), None);
    assert_eq!(game.position(fighter), at(4, 0, 0));
}

#[test]
fn a_move_cancels_a_windup_but_not_a_back_swing() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    let dummy = game.still(Team::new(1), at(2, 0, 0), dummy(100), false);

    // An attack starts in tick 0, in range; a move in tick 1 cancels it before its strike in
    // tick 2, and leaves the fighter ready: the attack ordered in tick 2 starts at once.
    game.tick(&[(0, &attack(fighter, dummy))]);
    assert_eq!(game.attack(fighter).started(), Some(0));
    game.tick(&[(0, &move_to(fighter, 0, 0))]);
    assert_eq!(game.attack(fighter), AttackState::default());
    game.tick(&[(0, &attack(fighter, dummy))]);
    assert_eq!(game.health(dummy), Some(100));
    assert_eq!(game.attack(fighter).started(), Some(2));
    game.run_until(5);
    assert_eq!(game.health(dummy), Some(70));
    assert_eq!(game.attack(fighter).ready_at(), 7);

    // A move in tick 5, after the strike, costs nothing: back to the dummy in tick 6, 2 m
    // away, the next attack starts in tick 7, when ready, and strikes in tick 9.
    game.tick(&[(0, &move_to(fighter, -1, 0))]);
    assert_eq!(game.position(fighter), at(-1, 0, 0));
    game.tick(&[(0, &attack(fighter, dummy))]);
    assert_eq!(game.position(fighter), at(0, 0, 0));
    game.run_until(9);
    assert_eq!(game.attack(fighter).started(), Some(7));
    assert_eq!(game.health(dummy), Some(70));
    game.run_until(10);
    assert_eq!(game.health(dummy), Some(40));
}

#[test]
fn attack_orders_need_a_living_enemy() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    let ally = game.hero(1, Team::new(0), at(1, 0, 0), fighter_stats());
    let gone = game.still(Team::new(1), at(9, 0, 0), dummy(30), false);
    let enemy = game.still(Team::new(1), at(1, 0, 0), dummy(100), false);

    // The fighter kills `gone` in tick 9: it reaches 7 m in tick 6, 2 m from it, starts an
    // attack in tick 7 and strikes 2 ticks later.
    game.tick(&[(0, &attack(fighter, gone))]);
    game.run_until(9);
    assert_eq!(game.health(gone), Some(30));
    game.run_until(10);
    assert_eq!(game.health(gone), None);

    for order in [
        attack(fighter, ally),
        attack(fighter, fighter),
        attack(fighter, gone),
    ] {
        game.tick(&[(0, &order)]);
        assert_eq!(game.attack(fighter).target(), None, "{order:?}");
    }
    game.tick(&[(0, &attack(fighter, enemy))]);
    assert_eq!(game.attack(fighter).target(), Some(enemy));

    // A dead hero takes no order, and no one can order an attack on it. The fighter, 1 m away,
    // starts in tick 0 and kills it in tick 2.
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    let doomed = game.hero(1, Team::new(1), at(1, 0, 0), combatant(10, 0, 0, 1, 0));
    game.tick(&[(0, &attack(fighter, doomed))]);
    game.run_until(3);
    assert!(game.dead(doomed));
    game.tick(&[(1, &move_to(doomed, 5, 5)), (0, &attack(fighter, doomed))]);
    assert_eq!(game.position(doomed), at(1, 0, 0));
    assert_eq!(game.attack(fighter).target(), None);
}

#[test]
fn a_tower_keeps_its_target_else_takes_the_nearest_enemy() {
    let mut game = Match::new();
    let tower = game.still(Team::new(1), at(0, 0, 0), dummy(100), true);
    let entity = game.world.resource::<EntityIndex>().get(tower).unwrap();
    *game.world.get_mut::<AttackStats>(entity).unwrap() =
        AttackStats::new(num(5), 1, 3, num(10)).unwrap();
    let prey = combatant(10, 0, 0, 1, 0);
    // On the ground plane the unit up at y = 9 is 3 m away, the nearest.
    let high = game.hero(0, Team::new(0), at(0, 9, 3), prey);
    let east = game.hero(1, Team::new(0), at(4, 0, 0), prey);
    let west = game.hero(2, Team::new(0), at(-4, 0, 0), prey);
    let edge = game.hero(3, Team::new(0), at(0, 0, 5), prey);
    let far = game.hero(4, Team::new(0), at(6, 0, 0), prey);
    let ally = game.hero(5, Team::new(1), at(1, 0, 0), prey);

    // Each target takes one strike: an attack every 3 ticks from tick 0, striking a tick later.
    // East and west tie at 4 m, and east has the lower id. Once west is the target, a nearer
    // enemy at 1 m spawns in tick 6: the tower keeps west, and takes the newcomer next.
    let mut targets = Vec::new();
    let mut newcomer = None;
    for tick in 0..15 {
        if tick == 6 {
            newcomer = Some(game.hero(6, Team::new(0), at(1, 0, 0), prey));
        }
        game.tick(&[]);
        targets.push(game.attack(tower).target());
    }
    let newcomer = newcomer.unwrap();
    let chosen = |target, ticks: usize| vec![Some(target); ticks];
    let expected = [
        chosen(high, 2),
        chosen(east, 3),
        chosen(west, 3),
        chosen(newcomer, 3),
        chosen(edge, 3),
        vec![None],
    ]
    .concat();
    assert_eq!(targets, expected);
    for (prey, died) in [(high, 1), (east, 4), (west, 7), (newcomer, 10), (edge, 13)] {
        assert!(game.dead(prey), "{prey:?} died in tick {died}");
    }
    for spared in [far, ally] {
        assert_eq!(game.health(spared), Some(10));
    }
}

#[test]
fn a_walker_follows_its_path_in_its_direction() {
    let path = [at(0, 0, 0), at(4, 0, 0), at(4, 0, 4)];
    let mut game = Match::with_lanes(Lanes::new([&path[..]]));
    let path_walker = |team, direction| {
        (
            dummy(10).bundle(team),
            meter().bundle(),
            LaneWalker::start(0, direction),
        )
    };
    let forward = game.spawn(
        at(0, 0, 0),
        path_walker(Team::new(0), PathDirection::Forward),
    );
    let backward = game.spawn(
        at(4, 0, 4),
        path_walker(Team::new(1), PathDirection::Backward),
    );

    // Each walks a meter a tick along the waypoints in its direction, then stays.
    let walked: Vec<_> = (0..9)
        .map(|_| {
            game.tick(&[]);
            [game.position(forward), game.position(backward)]
        })
        .collect();
    assert_eq!(
        walked,
        [
            [at(1, 0, 0), at(4, 0, 3)],
            [at(2, 0, 0), at(4, 0, 2)],
            [at(3, 0, 0), at(4, 0, 1)],
            [at(4, 0, 0), at(4, 0, 0)],
            [at(4, 0, 1), at(3, 0, 0)],
            [at(4, 0, 2), at(2, 0, 0)],
            [at(4, 0, 3), at(1, 0, 0)],
            [at(4, 0, 4), at(0, 0, 0)],
            [at(4, 0, 4), at(0, 0, 0)],
        ]
    );
}

#[test]
fn every_control_type_is_state_and_restores() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    game.still(Team::new(1), at(9, 0, 0), fighter_stats(), true);
    game.tick(&[(0, &move_to(fighter, 0, 3))]);

    let registry = &game.registry;
    let mut per_type = Vec::new();
    let hash = registry.hash_by_type(&game.world, &mut per_type);
    let names: Vec<_> = per_type.iter().map(|TypeHash { name, .. }| *name).collect();
    assert!(names.contains(&"control.controller") && names.contains(&"control.tower_ai"));

    let mut snapshot = Vec::new();
    registry.snapshot(&game.world, &mut snapshot);
    let mut restored = Match::new();
    registry.restore(&snapshot, &mut restored.world).unwrap();
    assert_eq!(registry.hash(&restored.world), hash);
    assert_eq!(restored.destination(fighter), Some(at(0, 0, 3)));
}
