use std::num::NonZeroU64;

use bevy_ecs::component::Component;
use campfire_math::SegmentSeed;
use campfire_sim::{SimUpdate, TickInput, TypeHash};

use super::*;

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

/// A unit of `health`, attacking within `range` for `damage`, with a windup and a period in
/// ticks; it walks a meter a tick, or never with `still`.
#[derive(Debug, Clone, Copy)]
struct Unit {
    health: i64,
    range: i64,
    windup: u32,
    period: u32,
    damage: i64,
    still: bool,
}

/// 100 health, and 30 damage within 2 m, 2 ticks after the start of an attack every 5 ticks.
const FIGHTER: Unit = Unit {
    health: 100,
    range: 2,
    windup: 2,
    period: 5,
    damage: 30,
    still: false,
};

/// A still target that never attacks.
const DUMMY: Unit = Unit {
    health: 100,
    range: 0,
    windup: 0,
    period: 1,
    damage: 0,
    still: true,
};

impl Unit {
    fn stats(self) -> UnitStats {
        UnitStats {
            health: Health::new(num(self.health)).unwrap(),
            attack: AttackStats::new(num(self.range), self.windup, self.period, num(self.damage))
                .unwrap(),
            step: (!self.still).then(|| MoveStep::new(Num::ONE).unwrap()),
        }
    }
}

#[derive(Debug)]
struct Match {
    world: World,
}

impl Match {
    fn new() -> Match {
        Match::with_map(Lanes::default(), None)
    }

    fn with_map(lanes: Lanes, waves: Option<Waves>) -> Match {
        let mut world = World::new();
        SimUpdate::prepare(&mut world, SegmentSeed::new([0; 32]));
        MobaKit::prepare(&mut world, lanes, waves);
        let mut schedule = SimUpdate::schedule();
        MobaKit::add_systems(&mut schedule);
        world.add_schedule(schedule);
        Match { world }
    }

    fn hero(&mut self, slot: u32, team: Team, at: Position, unit: Unit) -> StableId {
        MobaKit::spawn_hero(&mut self.world, slot, team, at, unit.stats())
    }

    fn tower(&mut self, team: Team, at: Position, unit: Unit) -> StableId {
        MobaKit::spawn_tower(&mut self.world, team, at, unit.stats())
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

    fn ids(&self) -> Vec<StableId> {
        self.world
            .resource::<EntityIndex>()
            .iter()
            .map(|(id, _)| id)
            .collect()
    }
}

fn move_to(x: i64, z: i64) -> Vec<u8> {
    Order::Move {
        x: num(x),
        z: num(z),
    }
    .encode()
}

fn attack(target: StableId) -> Vec<u8> {
    Order::Attack { target }.encode()
}

#[test]
fn orders_decode_exactly() {
    let order = Order::Move {
        x: num(-3),
        z: Num::from_bits(5),
    };
    let payload = order.encode();
    assert_eq!(Order::decode(&payload), Some(order));
    assert_eq!(Order::decode(&[payload.as_slice(), &[0]].concat()), None);
    assert_eq!(Order::decode(&payload[..payload.len() - 1]), None);
    assert_eq!(Order::decode(&[]), None);

    // Variant 1 with the target's id as a varint: 300 = 0xAC 0x02.
    let target = postcard::from_bytes::<StableId>(&[0xAC, 0x02]).unwrap();
    assert_eq!(target.get(), 300);
    assert_eq!(attack(target), [1, 0xAC, 0x02]);
    assert_eq!(
        Order::decode(&[1, 0xAC, 0x02]),
        Some(Order::Attack { target })
    );
    // Variant 2 does not exist.
    assert_eq!(Order::decode(&[2, 0]), None);
}

/// Heroes of slots 0 and 1, a meter a tick: slot 0 at the origin, slot 1 at x = 4 and y = 2.
fn two_heroes() -> (Match, [StableId; 2]) {
    let mut game = Match::new();
    let heroes = [
        game.hero(0, Team::First, at(0, 0, 0), FIGHTER),
        game.hero(1, Team::First, at(4, 2, 0), FIGHTER),
    ];
    (game, heroes)
}

fn destination(game: &Match, id: StableId) -> Option<Position> {
    game.get::<Destination>(id).unwrap().get()
}

#[test]
fn a_hero_walks_to_its_players_target() {
    let (mut game, [first, second]) = two_heroes();
    game.tick(&[(0, &move_to(0, 5))]);
    // Straight along z, one meter a tick: exact, and done in the fifth tick.
    assert_eq!(game.position(first), at(0, 0, 1));
    assert_eq!(destination(&game, first), Some(at(0, 0, 5)));
    assert_eq!(game.position(second), at(4, 2, 0));
    assert_eq!(destination(&game, second), None);
    for z in 2..5 {
        game.tick(&[]);
        assert_eq!(game.position(first), at(0, 0, z));
    }
    game.tick(&[]);
    assert_eq!(game.position(first), at(0, 0, 5));
    assert_eq!(destination(&game, first), None);
    game.tick(&[]);
    assert_eq!(game.position(first), at(0, 0, 5));

    // Offset (3, 0, 4) from slot 1's hero, distance 5: the first meter is 3/5 and 4/5, each
    // rounded once: 10 066 329.6 → 10 066 330 and 13 421 772.8 → 13 421 773. The order keeps
    // the hero's height, y = 2.
    game.tick(&[(1, &move_to(7, 4))]);
    assert_eq!(
        game.position(second),
        raw(4 * ONE + 10_066_330, 2 * ONE, 13_421_773)
    );
    assert_eq!(destination(&game, second), Some(at(7, 2, 4)));
    assert_eq!(game.position(first), at(0, 0, 5));
}

#[test]
fn the_last_order_in_a_tick_wins_and_bad_ones_are_ignored() {
    let (mut game, [first, second]) = two_heroes();
    game.tick(&[(0, &move_to(0, 5)), (0, &move_to(0, -5))]);
    assert_eq!(game.position(first), at(0, 0, -1));
    assert_eq!(destination(&game, first), Some(at(0, 0, -5)));

    let beyond = Order::Move {
        x: Position::BOUND + Num::EPSILON,
        z: Num::ZERO,
    }
    .encode();
    let edge = Order::Move {
        x: Position::BOUND,
        z: Num::ZERO,
    }
    .encode();
    game.tick(&[
        (0, &beyond),
        (0, b"not an order"),
        (2, &move_to(9, 9)),
        (1, &edge),
    ]);
    assert_eq!(game.position(first), at(0, 0, -2));
    assert_eq!(destination(&game, first), Some(at(0, 0, -5)));
    assert_eq!(
        destination(&game, second),
        Some(Position::new(Vec3::new(Position::BOUND, num(2), Num::ZERO)).unwrap())
    );
}

#[test]
fn an_attack_chases_winds_up_and_strikes_each_period() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::First, at(0, 0, 0), FIGHTER);
    let dummy = game.tower(Team::Second, at(6, 0, 0), DUMMY);

    // Ticks 0 to 3 walk 4 m, to 2 m from the dummy. Tick 4 starts an attack: it strikes in tick
    // 6, and the next may start in tick 4 + 5 = 9.
    game.tick(&[(0, &attack(dummy))]);
    game.run_until(4);
    assert_eq!(game.position(fighter), at(4, 0, 0));
    assert_eq!(game.attack(fighter).started(), None);
    game.run_until(6);
    assert_eq!(game.attack(fighter).started(), Some(4));
    assert_eq!(game.health(dummy), Some(100));
    game.run_until(7);
    assert_eq!(game.health(dummy), Some(70));
    assert_eq!(game.attack(fighter).ready_at(), 9);
    assert_eq!(destination(&game, fighter), None);

    // Strikes in ticks 11, 16 and 21 take 70 to 40, 10 and 0: the dummy despawns in tick 21.
    game.run_until(21);
    assert_eq!(game.health(dummy), Some(10));
    game.run_until(22);
    assert_eq!(game.health(dummy), None);
    assert_eq!(game.attack(fighter).target(), Some(dummy));
    // The next tick finds the target gone and drops it.
    game.run_until(23);
    assert_eq!(game.attack(fighter).target(), None);
    assert_eq!(game.position(fighter), at(4, 0, 0));
}

#[test]
fn a_move_cancels_a_windup_but_not_a_back_swing() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::First, at(0, 0, 0), FIGHTER);
    let dummy = game.tower(Team::Second, at(2, 0, 0), DUMMY);

    // An attack starts in tick 0, in range; a move in tick 1 cancels it before its strike in
    // tick 2, and leaves the fighter ready: the attack ordered in tick 2 starts at once.
    game.tick(&[(0, &attack(dummy))]);
    assert_eq!(game.attack(fighter).started(), Some(0));
    game.tick(&[(0, &move_to(0, 0))]);
    assert_eq!(game.attack(fighter), AttackState::default());
    game.tick(&[(0, &attack(dummy))]);
    assert_eq!(game.health(dummy), Some(100));
    assert_eq!(game.attack(fighter).started(), Some(2));
    game.run_until(5);
    assert_eq!(game.health(dummy), Some(70));
    assert_eq!(game.attack(fighter).ready_at(), 7);

    // A move in tick 5, after the strike, costs nothing: back to the dummy in tick 6, 2 m
    // away, the next attack starts in tick 7, when ready, and strikes in tick 9.
    game.tick(&[(0, &move_to(-1, 0))]);
    assert_eq!(game.position(fighter), at(-1, 0, 0));
    game.tick(&[(0, &attack(dummy))]);
    assert_eq!(game.position(fighter), at(0, 0, 0));
    game.run_until(9);
    assert_eq!(game.attack(fighter).started(), Some(7));
    assert_eq!(game.health(dummy), Some(70));
    game.run_until(10);
    assert_eq!(game.health(dummy), Some(40));
}

#[test]
fn strikes_in_one_tick_see_the_state_before_any_of_them() {
    let mut game = Match::new();
    let duelist = Unit {
        health: 30,
        windup: 1,
        ..FIGHTER
    };
    let first = game.hero(0, Team::First, at(0, 0, 0), duelist);
    let second = game.hero(1, Team::Second, at(1, 0, 0), duelist);

    // Both attacks start in tick 0 and strike in tick 1: each kills the other.
    game.tick(&[(0, &attack(second)), (1, &attack(first))]);
    game.tick(&[]);
    for hero in [first, second] {
        assert_eq!(game.health(hero), Some(0));
        assert!(game.dead(hero));
        assert_eq!(game.attack(hero).target(), None);
    }

    // A dead hero takes no order, and no one can attack it.
    let third = game.hero(2, Team::Second, at(3, 0, 0), FIGHTER);
    game.tick(&[(0, &move_to(0, 9)), (2, &attack(first))]);
    assert_eq!(game.position(first), at(0, 0, 0));
    assert_eq!(game.attack(third).target(), None);
}

#[test]
fn attack_orders_need_a_living_enemy() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::First, at(0, 0, 0), FIGHTER);
    let ally = game.hero(1, Team::First, at(1, 0, 0), FIGHTER);
    let gone = game.tower(
        Team::Second,
        at(9, 0, 0),
        Unit {
            health: 30,
            ..DUMMY
        },
    );
    let enemy = game.tower(Team::Second, at(1, 0, 0), DUMMY);

    // The fighter kills `gone` in tick 9: it reaches 7 m in tick 6, 2 m from it, starts an
    // attack in tick 7 and strikes 2 ticks later.
    game.tick(&[(0, &attack(gone))]);
    game.run_until(9);
    assert_eq!(game.health(gone), Some(30));
    game.run_until(10);
    assert_eq!(game.health(gone), None);

    for order in [attack(ally), attack(fighter), attack(gone)] {
        game.tick(&[(0, &order)]);
        assert_eq!(game.attack(fighter).target(), None, "{order:?}");
    }
    game.tick(&[(0, &attack(enemy))]);
    assert_eq!(game.attack(fighter).target(), Some(enemy));
}

#[test]
fn a_tower_keeps_its_target_else_takes_the_nearest_enemy() {
    let mut game = Match::new();
    let tower = game.tower(
        Team::Second,
        at(0, 0, 0),
        Unit {
            range: 5,
            windup: 1,
            period: 3,
            damage: 10,
            ..DUMMY
        },
    );
    let prey = Unit {
        health: 10,
        ..DUMMY
    };
    // On the ground plane the unit up at y = 9 is 3 m away, the nearest.
    let high = game.hero(0, Team::First, at(0, 9, 3), prey);
    let east = game.hero(1, Team::First, at(4, 0, 0), prey);
    let west = game.hero(2, Team::First, at(-4, 0, 0), prey);
    let edge = game.hero(3, Team::First, at(0, 0, 5), prey);
    let far = game.hero(4, Team::First, at(6, 0, 0), prey);
    let ally = game.hero(5, Team::Second, at(1, 0, 0), prey);

    // Each target takes one strike: an attack every 3 ticks from tick 0, striking a tick later.
    // East and west tie at 4 m, and east has the lower id. Once west is the target, a nearer
    // enemy at 1 m spawns in tick 6: the tower keeps west, and takes the newcomer next.
    let mut targets = Vec::new();
    let mut newcomer = None;
    for tick in 0..15 {
        if tick == 6 {
            newcomer = Some(game.hero(6, Team::First, at(1, 0, 0), prey));
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
fn creeps_spawn_in_waves_and_walk_their_lane() {
    let path = [at(0, 0, 0), at(4, 0, 0), at(4, 0, 4)];
    let lanes = Lanes::new([&path[..]]);
    let creep = Unit {
        health: 10,
        range: 1,
        windup: 0,
        period: 1,
        damage: 0,
        still: false,
    };
    let waves = Waves {
        first: 1,
        interval: NonZeroU64::new(10).unwrap(),
        creeps: vec![creep.stats()],
    };
    let mut game = Match::with_map(lanes, Some(waves));
    game.tick(&[]);
    assert!(game.ids().is_empty());

    // Tick 1 spawns the first side's creep at the lane's start and the second's at its end;
    // each walks a meter a tick along the waypoints in its side's order, then stays.
    game.tick(&[]);
    let [forward, backward] = [0, 1].map(|id| postcard::from_bytes::<StableId>(&[id]).unwrap());
    assert_eq!(game.ids(), [forward, backward]);
    let walked: Vec<_> = (2..11)
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
    assert_eq!(game.ids().len(), 2);

    // The next wave comes 10 ticks after the first.
    game.tick(&[]);
    let ids = game.ids();
    assert_eq!(ids.len(), 4);
    assert_eq!(game.position(ids[2]), at(0, 0, 0));
    assert_eq!(game.position(ids[3]), at(4, 0, 4));
}

#[test]
fn lanes_give_each_side_its_order() {
    let lanes = Lanes::new([&[at(0, 0, 0), at(1, 0, 0)][..], &[at(5, 0, 5)][..]]);
    assert_eq!(lanes.count(), 2);
    let walk = |lane, team| {
        (0..3)
            .map(|index| lanes.waypoint(lane, index, team))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        walk(0, Team::First),
        [Some(at(0, 0, 0)), Some(at(1, 0, 0)), None]
    );
    assert_eq!(
        walk(0, Team::Second),
        [Some(at(1, 0, 0)), Some(at(0, 0, 0)), None]
    );
    assert_eq!(walk(1, Team::Second), [Some(at(5, 0, 5)), None, None]);
    assert_eq!(walk(0, Team::Neutral), [None, None, None]);
    assert_eq!(walk(2, Team::First), [None, None, None]);

    // Different teams are enemies; the neutral team is an enemy of both sides.
    for (a, b, enemies) in [
        (Team::First, Team::Second, true),
        (Team::First, Team::Neutral, true),
        (Team::Second, Team::Neutral, true),
        (Team::First, Team::First, false),
        (Team::Neutral, Team::Neutral, false),
    ] {
        assert_eq!(a.is_enemy_of(b), enemies, "{a:?} {b:?}");
        assert_eq!(b.is_enemy_of(a), enemies, "{b:?} {a:?}");
    }
}

#[test]
fn every_kit_type_is_state_and_restores() {
    let mut game = Match::with_map(
        Lanes::new([&[at(0, 0, 0), at(9, 0, 0)][..]]),
        Some(Waves {
            first: 0,
            interval: NonZeroU64::MIN,
            creeps: vec![DUMMY.stats()],
        }),
    );
    let fighter = game.hero(0, Team::First, at(0, 0, 0), FIGHTER);
    game.tower(Team::Second, at(9, 0, 0), FIGHTER);
    let doomed = game.hero(1, Team::Second, at(0, 0, 1), Unit { health: 1, ..DUMMY });
    game.tick(&[(0, &attack(doomed))]);
    game.run_until(3);
    assert!(game.dead(doomed));

    let mut registry = StateRegistry::new();
    MobaKit::register_state(&mut registry);
    let mut per_type = Vec::new();
    let hash = registry.hash_by_type(&game.world, &mut per_type);
    let names: Vec<_> = per_type.iter().map(|TypeHash { name, .. }| *name).collect();
    assert_eq!(
        names,
        [
            "moba.attack",
            "moba.attack_stats",
            "moba.controller",
            "moba.dead",
            "moba.destination",
            "moba.health",
            "moba.lane_walker",
            "moba.move_step",
            "moba.team",
            "moba.tower_ai",
            "sim.entities",
            "sim.id_allocator",
            "sim.position",
            "sim.tick",
        ]
    );

    let mut snapshot = Vec::new();
    registry.snapshot(&game.world, &mut snapshot);
    let mut restored = Match::new();
    registry.restore(&snapshot, &mut restored.world).unwrap();
    assert_eq!(registry.hash(&restored.world), hash);
    assert_eq!(restored.attack(fighter), game.attack(fighter));
}

#[test]
fn stats_out_of_their_limits_are_refused() {
    assert_eq!(Health::new(Num::ZERO), None);
    assert_eq!(
        Health::new(Num::EPSILON).map(Health::current),
        Some(Num::EPSILON)
    );
    assert_eq!(AttackStats::new(-Num::EPSILON, 0, 1, Num::ZERO), None);
    assert_eq!(AttackStats::new(Num::ZERO, 0, 1, -Num::EPSILON), None);
    assert_eq!(AttackStats::new(Num::ZERO, 1, 1, Num::ZERO), None);
    assert!(AttackStats::new(Num::ZERO, 0, 1, Num::ZERO).is_some());

    // A snapshot's values pass the same limits.
    let health = |current: i64, max: i64| {
        let bytes = postcard::to_allocvec(&(num(current), num(max))).unwrap();
        postcard::from_bytes::<Health>(&bytes).ok()
    };
    assert!(health(0, 1).is_some() && health(1, 1).is_some());
    for (current, max) in [(-1, 1), (2, 1), (0, 0)] {
        assert_eq!(health(current, max), None, "{current} of {max}");
    }
    let stats = |windup: u32, period: u32| {
        let bytes = postcard::to_allocvec(&(Num::ONE, windup, period, Num::ONE)).unwrap();
        postcard::from_bytes::<AttackStats>(&bytes).ok()
    };
    assert_eq!(stats(1, 2), AttackStats::new(Num::ONE, 1, 2, Num::ONE));
    assert_eq!(stats(2, 2), None);
}

#[test]
fn move_steps_are_never_negative() {
    assert_eq!(MoveStep::new(-Num::EPSILON), None);
    assert_eq!(MoveStep::new(Num::ZERO).map(MoveStep::get), Some(Num::ZERO));
    let negative = postcard::to_allocvec(&-Num::EPSILON).unwrap();
    assert!(postcard::from_bytes::<MoveStep>(&negative).is_err());
    let one = postcard::to_allocvec(&Num::ONE).unwrap();
    assert_eq!(
        postcard::from_bytes::<MoveStep>(&one).ok(),
        MoveStep::new(Num::ONE)
    );
}
