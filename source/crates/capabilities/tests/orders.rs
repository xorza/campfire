use std::num::NonZeroU64;

use bevy_ecs::change_detection::DetectChanges;
use campfire_actions::internals::TestWeapon;
use campfire_actions::{
    ActionRange, ActionSlots, ActionTarget, Actions, InProgress, OrderPhase, SlotAim, SlotKind,
    SlotTimes, TimedSlots,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Order, Release};
use campfire_codec::Binary;
use campfire_combat::internals::Armed;
use campfire_combat::{Combat, OnDeath};
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_deliveries::Deliveries;
use campfire_geometry::{Bounds, Grid};
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_navigation::{
    Navigation, OnPath, PathEnd, PathWalker, Paths, Route, RoutePlanner, Walker,
};
use campfire_orders::*;
use campfire_players::{OutOfPlay, Players, PlayersWorld};
use campfire_script::internals::FailureKind;
use campfire_script::{
    ApiError, CallError, Filter, Hook, ScriptBudgets, ScriptError, ScriptFailures, ScriptLimits,
    TagNames,
};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Command, IdAllocator, Position, StableId, Unpredicted};
use campfire_stats::{Life, Stats};
use campfire_units::{
    BodyForm, Dead, DeliveryEntity, Destination, Layer, MoveStep, Owner, UnitTypeCore, Units,
};
use campfire_values::{PackagePath, Rank, Scalar};
use campfire_values::{PathId, Team, TeamSet, UnitType};
use campfire_visibility::OutView;
use campfire_vision::{SeenBy, Vision};

const ONE: i64 = 1 << 24;
/// The MOBA 3v3's creep and tower AI as they were when these tests were written: the
/// engine's tests keep their own copies, so a change to the MOBA's mode changes none of them.
const CREEP_AI: &str = r#"
fn on_think(ctx, unit) {
    let target = defend_hero(ctx, unit);
    if target == () {
        target = keep_target(unit);
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:creep");
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:avatar");
    }
    if target == () {
        target = ctx.nearest_visible(unit, unit.params.aggro_range, "enemies:structure");
    }
    if target == () {
        ctx.order_follow_path(unit);
    } else if target != unit.target {
        ctx.order_attack(unit, target);
    }
}

fn defend_hero(ctx, unit) {
    for ally in ctx.find(unit, unit.pos, unit.params.help_range, "allies:avatar") {
        for attacker in ally.recent_attackers(unit.params.help_window_ms) {
            if attacker.is_avatar && attacker.is_enemy_of(unit) && in_reach(unit, attacker) {
                return attacker;
            }
        }
    }
    ()
}

fn keep_target(unit) {
    let target = unit.target;
    if target != () && target.alive && in_reach(unit, target) {
        target
    } else {
        ()
    }
}

fn in_reach(unit, other) {
    unit.can_see(other) && unit.pos.within(other.pos, unit.params.aggro_range)
}
"#;
const TOWER_AI: &str = r#"
fn on_think(ctx, tower) {
    let range = tower.attack_range;
    let target = defend_hero(ctx, tower, range);
    if target == () {
        let current = tower.target;
        if current != () && current.alive && tower.pos.within(current.pos, range) {
            target = current;
        }
    }
    if target == () {
        target = ctx.nearest_visible(tower, range, "enemies:creep");
    }
    if target == () {
        target = ctx.nearest_visible(tower, range, "enemies:avatar");
    }
    if target != () && target != tower.target {
        ctx.order_attack(tower, target);
    }
}

fn defend_hero(ctx, tower, range) {
    for ally in ctx.find(tower, tower.pos, range, "allies:avatar") {
        for attacker in ally.recent_attackers(tower.params.help_window_ms) {
            if attacker.is_avatar && attacker.is_enemy_of(tower) && tower.pos.within(attacker.pos, range) {
                return attacker;
            }
        }
    }
    ()
}
"#;

const CAMP_AI: &str = "
fn on_think(ctx, unit) {
    let away = unit.pos.distance_to(unit.spawn_pos);
    if away > unit.params.leash_range {
        ctx.order_reset(unit);
        return;
    }
    let target = unit.target;
    if target != () && target.alive && unit.can_see(target) {
        return;
    }
    for attacker in unit.recent_attackers(unit.params.aggro_window_ms) {
        if unit.can_see(attacker) {
            ctx.order_attack(unit, attacker);
            return;
        }
    }
    if away > unit.params.home_slack {
        ctx.order_move(unit, unit.spawn_pos);
    }
}
";

const fn raw(x: i64, y: i64, z: i64) -> Position {
    Position::new(Vec3::new(
        Num::from_bits(x),
        Num::from_bits(y),
        Num::from_bits(z),
    ))
    .unwrap()
}

/// `health`, and `damage` within `range`, a windup and a period in ticks; `Stay` on death, as a
/// hero.
const fn combatant(health: i64, range: i64, windup: u64, period: u64, damage: i64) -> Armed {
    Armed::melee(
        Num::int(health),
        Num::int(range),
        windup,
        period,
        Num::int(damage),
    )
    .on_death(OnDeath::Stay)
}

/// 100 health, and 30 damage within 2 m, 2 ticks after the start of an attack every 5 ticks.
const fn fighter_stats() -> Armed {
    combatant(100, 2, 2, 5, 30)
}

/// A still target that never attacks and despawns when it dies.
const fn dummy(health: i64) -> Armed {
    Armed::unarmed(Num::from_bits(health << 24))
}

const fn meter() -> MoveStep {
    MoveStep::new(Num::ONE).unwrap()
}

fn orders_match() -> Match {
    orders_match_with_paths(Paths::default())
}

fn orders_match_with_paths(paths: Paths) -> Match {
    let limits = ScriptLimits::ROOMY;
    orders_match_with(paths, limits)
}

/// The capabilities of an orders match.
const ORDERS: [Capability; 4] = [
    Capability::Stats,
    Capability::Combat,
    Capability::Navigation,
    Capability::Orders,
];

/// A match of `ORDERS` with `paths`, whose scripts run within `limits`, and the stat book a
/// weapon reads.
fn orders_match_with(paths: Paths, limits: ScriptLimits) -> Match {
    let scripts = ScriptBudgets::new(limits, 2);
    let mut game = Match::server(&ORDERS, scripts);
    Navigation::load_paths(&mut game, paths);
    Combat::load_weapon_stats(&mut game);
    game
}

/// Gives `unit_type` the combat `armed`.
fn arm(game: &mut Match, unit_type: UnitType, armed: Armed) -> UnitType {
    armed.give(game, unit_type);
    unit_type
}

/// The next type of the mode, armed as `armed`, walking `step` a tick when given.
fn armed(game: &mut Match, armed: Armed, step: Option<MoveStep>) -> UnitType {
    let unit_type = armed.load(game);
    if let Some(step) = step {
        Units::give_type(game, unit_type, step);
    }
    unit_type
}

/// A spawn group of a walker of its own, which shares a route with none.
fn lone() -> StableId {
    IdAllocator::default().allocate()
}

fn move_to(id: StableId, x: i64, z: i64) -> Vec<u8> {
    Order::payload(&[Order::one(
        id,
        OrdersAction::Move {
            x: Num::int(x),
            z: Num::int(z),
        },
    )])
}

fn attack(id: StableId, target: StableId) -> Vec<u8> {
    Order::payload(&[Order::one(id, OrdersAction::Attack { target })])
}

/// The heroes of slots 0 and 1, a meter a tick: slot 0 at the origin, slot 1 at x = 4 and y = 2.
#[derive(Debug, Clone, Copy)]
struct TwoHeroes {
    first: StableId,
    second: StableId,
}

impl TwoHeroes {
    /// Spawns the heroes into `game`, an orders match.
    fn place(game: &mut Match) -> TwoHeroes {
        let hero = armed(game, fighter_stats(), Some(meter()));
        TwoHeroes {
            first: Orders::hero(game, hero, (0, 0, 0), Team::new(0), 0),
            second: Orders::hero(game, hero, (4, 2, 0), Team::new(0), 1),
        }
    }
}

#[test]
fn a_hero_walks_to_its_players_target() {
    let mut game = orders_match();
    let TwoHeroes { first, second } = TwoHeroes::place(&mut game);
    game.step_with(&[(PlayerSlot::new(0), &move_to(first, 0, 5))]);
    // Straight along z, one meter a tick: exact, and done in the fifth tick.
    assert_eq!(*game.part::<Position>(first), Position::from((0, 0, 1)));
    assert_eq!(
        game.part::<Destination>(first).get(),
        Some(Position::from((0, 0, 5)))
    );
    assert_eq!(*game.part::<Position>(second), Position::from((4, 2, 0)));
    assert_eq!(game.part::<Destination>(second).get(), None);
    game.run_until(5);
    assert_eq!(*game.part::<Position>(first), Position::from((0, 0, 5)));
    assert_eq!(game.part::<Destination>(first).get(), None);

    // Offset (3, 0, 4) from slot 1's hero, distance 5: the first meter is 3/5 and 4/5, each
    // rounded once: 10 066 329.6 → 10 066 330 and 13 421 772.8 → 13 421 773. The order keeps
    // the hero's height, y = 2.
    game.step_with(&[(PlayerSlot::new(1), &move_to(second, 7, 4))]);
    assert_eq!(
        *game.part::<Position>(second),
        raw(4 * ONE + 10_066_330, 2 * ONE, 13_421_773)
    );
    assert_eq!(
        game.part::<Destination>(second).get(),
        Some(Position::from((7, 2, 4)))
    );
}

/// The heroes of `TwoHeroes`, placed into `game`, the first with its weapon in slot 0 and a train
/// in slot 1.
fn trainer(game: &mut Match) -> TwoHeroes {
    let heroes = TwoHeroes::place(game);
    let train = Actions::add_train(game, UnitType::new(0), Ticks::ZERO, None);
    let entity = game.unit_entity(heroes.first);
    let mut unit = game.entity_mut(entity);
    let (mut slots, mut times) = unit
        .get_components_mut::<(&mut ActionSlots, &mut SlotTimes)>()
        .unwrap();
    slots.grant(&mut times, SlotKind::new(0), &[train], Some(Rank::FIRST));
    heroes
}

#[test]
fn a_slot_order_starts_a_cast_or_a_train_and_no_other_kind() {
    let mut game = orders_match();
    let TwoHeroes {
        first: hero,
        second: other,
    } = trainer(&mut game);
    let ordered = |game: &Match| game.part::<ActionSlots>(hero).in_progress();
    let slot = |slot| {
        Order::payload(&[Order::one(
            hero,
            OrdersAction::Slot {
                slot,
                target: ActionTarget::None,
            },
        )])
    };
    // The weapon's slot and a slot it does not have order nothing; the train's orders a train,
    // which no capability here starts.
    let train_order = InProgress::Order {
        aim: SlotAim {
            slot: 1,
            target: ActionTarget::None,
        },
        phase: OrderPhase::Ordered,
    };
    for (at, order) in [(0, None), (7, None), (1, Some(train_order))] {
        game.step_with(&[(PlayerSlot::new(0), &slot(at))]);
        assert_eq!(ordered(&game), order, "slot {at}");
    }

    // With a vision grid, the train at a unit: at the other hero while no team sees it, nothing
    // changes; at the hero itself, of its own team with no `SeenBy`, and at the other once team 0
    // sees it, the order takes its target.
    Vision::fog_grid(&mut game, 1);
    game.insert_parts(other, SeenBy::new(TeamSet::NONE));
    let at_unit = |unit| {
        Order::payload(&[Order::one(
            hero,
            OrdersAction::Slot {
                slot: 1,
                target: ActionTarget::Unit(unit),
            },
        )])
    };
    let aimed = |unit| InProgress::Order {
        aim: SlotAim {
            slot: 1,
            target: ActionTarget::Unit(unit),
        },
        phase: OrderPhase::Ordered,
    };
    game.step_with(&[(PlayerSlot::new(0), &at_unit(other))]);
    assert_eq!(ordered(&game), Some(train_order));
    // A player's entity of the hero's team holds a team and no `SeenBy`, and is no unit: an order
    // at it changes nothing either.
    let own = PlayerSlot::new(0);
    Players::spawn(&mut game, 1, 0);
    game.step_with(&[(PlayerSlot::new(0), &at_unit(StableId::of_player(own)))]);
    assert_eq!(ordered(&game), Some(train_order));
    game.step_with(&[(PlayerSlot::new(0), &at_unit(hero))]);
    assert_eq!(ordered(&game), Some(aimed(hero)));
    game.insert_parts(other, SeenBy::new(TeamSet::of(Team::new(0))));
    game.step_with(&[(PlayerSlot::new(0), &at_unit(other))]);
    assert_eq!(ordered(&game), Some(aimed(other)));

    // A predicting client holds only what its team sees, and takes each unit it holds as seen:
    // the train at the other hero, which no team's vision sees and which the client holds as the
    // server sent it, takes its target, where the server above dropped it.
    let mut client = Match::predicting(&ORDERS);
    Combat::load_weapon_stats(&mut client);
    let TwoHeroes { second: held, .. } = trainer(&mut client);
    assert_eq!(held, other);
    Vision::fog_grid(&mut client, 1);
    client.insert_parts(held, (SeenBy::new(TeamSet::NONE), Unpredicted));
    client.step_with(&[(PlayerSlot::new(0), &at_unit(held))]);
    assert_eq!(ordered(&client), Some(aimed(held)));
}

#[test]
fn only_its_players_orders_in_the_orders_capability_move_a_unit() {
    let mut game = orders_match();
    let TwoHeroes { first, second } = TwoHeroes::place(&mut game);
    game.step_with(&[
        (PlayerSlot::new(0), &move_to(first, 0, 5)),
        (PlayerSlot::new(0), &move_to(first, 0, -5)),
    ]);
    // The last order in a tick wins.
    assert_eq!(*game.part::<Position>(first), Position::from((0, 0, -1)));
    assert_eq!(
        game.part::<Destination>(first).get(),
        Some(Position::from((0, 0, -5)))
    );

    // Past the world's bound, which bounds a match without a map: it clamps to the edge.
    let beyond = Order::payload(&[Order::one(
        first,
        OrdersAction::Move {
            x: Position::BOUND + Num::EPSILON,
            z: -Num::int(1),
        },
    )]);
    let edge = Order::payload(&[Order::one(
        second,
        OrdersAction::Move {
            x: Position::BOUND,
            z: Num::ZERO,
        },
    )]);
    let not_an_order = Command::encode(&[Command {
        capability: Capability::Orders,
        body: b"not an order",
    }]);
    // A valid order, sent to a capability the mode did not declare.
    let order = Order::one(
        first,
        OrdersAction::Move {
            x: Num::int(9),
            z: Num::int(9),
        },
    )
    .encode();
    let undeclared = Command::encode(&[Command {
        capability: Capability::Character,
        body: &order,
    }]);
    game.step_with(&[
        (PlayerSlot::new(0), &beyond),
        (PlayerSlot::new(0), &not_an_order),
        (PlayerSlot::new(0), &undeclared),
        (PlayerSlot::new(0), &order),
        (PlayerSlot::new(0), &move_to(second, 9, 9)),
        (PlayerSlot::new(2), &move_to(first, 9, 9)),
        (PlayerSlot::new(1), &edge),
    ]);
    // Only the order past the bound reaches the first hero, which walks 1 m along z = −1 towards
    // the edge; the second takes only its own player's.
    assert_eq!(*game.part::<Position>(first), Position::from((1, 0, -1)));
    assert_eq!(
        game.part::<Destination>(first).get(),
        Some(Position::new(Vec3::new(Position::BOUND, Num::ZERO, -Num::int(1))).unwrap())
    );
    assert_eq!(
        game.part::<Destination>(second).get(),
        Some(Position::new(Vec3::new(Position::BOUND, Num::int(2), Num::ZERO)).unwrap())
    );

    // Player 0 out of play: its order applies while it sees what its team sees, the default, and
    // none does once it sees what a spectator sees; player 1's applies either way.
    assert_eq!(*game.resource::<OutView>(), OutView::Team);
    Players::spawn(&mut game, 2, 0);
    let out = game.player_entity(PlayerSlot::new(0)).unwrap();
    game.entity_mut(out).insert(OutOfPlay);
    let destination = |game: &Match, unit| game.part::<Destination>(unit).get();
    for (view, x) in [(OutView::Team, 3_i16), (OutView::Spectators, 6)] {
        game.insert_resource(view);
        let before = destination(&game, first);
        game.step_with(&[
            (PlayerSlot::new(0), &move_to(first, x.into(), 3)),
            (PlayerSlot::new(1), &move_to(second, x.into(), 3)),
        ]);
        let ordered = Some(Position::from((x, 0, 3)));
        let first_to = if view == OutView::Team {
            ordered
        } else {
            before
        };
        assert_eq!(destination(&game, first), first_to, "{view:?}");
        assert_eq!(
            destination(&game, second),
            Some(Position::from((x, 2, 3))),
            "{view:?}"
        );
    }
}

#[test]
fn an_attack_chases_winds_up_and_strikes_each_period() {
    let mut game = orders_match();
    let hero = armed(&mut game, fighter_stats(), Some(meter()));
    let dummy = armed(&mut game, dummy(100), None);
    let fighter = Orders::hero(&mut game, hero, (0, 0, 0), Team::new(0), 0);
    let dummy = Mode::spawn(&mut game, dummy, (6, 0, 0), Team::new(1), ());

    // Ticks 0 to 3 walk 4 m, to 2 m from the dummy. Tick 4 starts an attack: it strikes in tick
    // 6, and the next may start in tick 4 + 5 = 9.
    game.step_with(&[(PlayerSlot::new(0), &attack(fighter, dummy))]);
    game.run_until(4);
    assert_eq!(*game.part::<Position>(fighter), Position::from((4, 0, 0)));
    assert_eq!(Orders::strikes_at(&game, fighter), None);
    game.run_until(6);
    assert_eq!(Orders::strikes_at(&game, fighter), Some(Tick::new(6)));
    assert_eq!(Stats::health(&game, dummy), 100);
    game.run_until(7);
    assert_eq!(Stats::health(&game, dummy), 70);
    assert_eq!(Orders::ready_at(&game, fighter), Tick::new(9));
    assert_eq!(game.part::<Destination>(fighter).get(), None);

    // Strikes in ticks 11, 16 and 21 take 70 to 40, 10 and 0: the dummy despawns in tick 21.
    game.run_until(21);
    assert_eq!(Stats::health(&game, dummy), 10);
    game.run_until(22);
    assert!(game.try_part::<Life>(dummy).is_none());
    // The next tick finds the target gone, drops it, and stays.
    game.run_until(23);
    assert_eq!(game.part::<ActionSlots>(fighter).attack_target(), None);
    assert_eq!(*game.part::<Position>(fighter), Position::from((4, 0, 0)));
}

#[test]
fn a_move_cancels_a_windup_but_not_a_back_swing() {
    let mut game = orders_match();
    let hero = armed(&mut game, fighter_stats(), Some(meter()));
    let dummy = armed(&mut game, dummy(100), None);
    let fighter = Orders::hero(&mut game, hero, (0, 0, 0), Team::new(0), 0);
    let dummy = Mode::spawn(&mut game, dummy, (2, 0, 0), Team::new(1), ());

    // An attack starts in tick 0, in range; a move in tick 1 cancels it before its strike in
    // tick 2, and leaves the fighter ready: the attack ordered in tick 2 starts at once.
    game.step_with(&[(PlayerSlot::new(0), &attack(fighter, dummy))]);
    assert_eq!(Orders::strikes_at(&game, fighter), Some(Tick::new(2)));
    game.step_with(&[(PlayerSlot::new(0), &move_to(fighter, 0, 0))]);
    assert_eq!(game.part::<ActionSlots>(fighter).attack_target(), None);
    assert_eq!(Orders::strikes_at(&game, fighter), None);
    assert_eq!(Orders::ready_at(&game, fighter), Tick::new(0));
    game.step_with(&[(PlayerSlot::new(0), &attack(fighter, dummy))]);
    assert_eq!(Stats::health(&game, dummy), 100);
    assert_eq!(Orders::strikes_at(&game, fighter), Some(Tick::new(4)));
    game.run_until(5);
    assert_eq!(Stats::health(&game, dummy), 70);
    assert_eq!(Orders::ready_at(&game, fighter), Tick::new(7));

    // A move in tick 5, after the strike, costs nothing: back to the dummy in tick 6, 2 m
    // away, the next attack starts in tick 7, when ready, and strikes in tick 9.
    game.step_with(&[(PlayerSlot::new(0), &move_to(fighter, -1, 0))]);
    assert_eq!(*game.part::<Position>(fighter), Position::from((-1, 0, 0)));
    game.step_with(&[(PlayerSlot::new(0), &attack(fighter, dummy))]);
    assert_eq!(*game.part::<Position>(fighter), Position::from((0, 0, 0)));
    game.run_until(9);
    assert_eq!(Orders::strikes_at(&game, fighter), Some(Tick::new(9)));
    assert_eq!(Stats::health(&game, dummy), 70);
    game.run_until(10);
    assert_eq!(Stats::health(&game, dummy), 40);
}

#[test]
fn attack_orders_need_a_living_enemy() {
    let mut game = orders_match();
    let hero = armed(&mut game, fighter_stats(), Some(meter()));
    let [weak, strong] = [30, 100].map(|health| armed(&mut game, dummy(health), None));
    let fighter = Orders::hero(&mut game, hero, (0, 0, 0), Team::new(0), 0);
    let ally = Orders::hero(&mut game, hero, (1, 0, 0), Team::new(0), 1);
    let gone = Mode::spawn(&mut game, weak, (9, 0, 0), Team::new(1), ());
    let enemy = Mode::spawn(&mut game, strong, (1, 0, 0), Team::new(1), ());

    // The fighter kills `gone` in tick 9: it reaches 7 m in tick 6, 2 m from it, starts an
    // attack in tick 7 and strikes 2 ticks later.
    game.step_with(&[(PlayerSlot::new(0), &attack(fighter, gone))]);
    game.run_until(9);
    assert_eq!(Stats::health(&game, gone), 30);
    game.run_until(10);
    assert!(game.try_part::<Life>(gone).is_none());

    for order in [
        attack(fighter, ally),
        attack(fighter, fighter),
        attack(fighter, gone),
    ] {
        game.step_with(&[(PlayerSlot::new(0), &order)]);
        assert_eq!(
            game.part::<ActionSlots>(fighter).attack_target(),
            None,
            "{order:?}"
        );
    }

    // With a vision grid, an attack aims only at a unit the fighter's team sees as the order
    // applies. The enemy has no `SeenBy`, as a unit spawned after the last Vision stage, so its
    // own team alone sees it; then team 1 alone sees it; then team 0 does too.
    Vision::fog_grid(&mut game, 2);
    let enemies = TeamSet::of(Team::new(1));
    for seers in [None, Some(enemies), Some(enemies.with(Team::new(0)))] {
        if let Some(seers) = seers {
            game.insert_parts(enemy, SeenBy::new(seers));
        }
        game.step_with(&[(PlayerSlot::new(0), &attack(fighter, enemy))]);
        let expected = seers.is_some_and(|seers| seers.contains(Team::new(0)));
        assert_eq!(
            game.part::<ActionSlots>(fighter).attack_target(),
            expected.then_some(enemy),
            "{seers:?}"
        );
    }

    // A dead hero takes no order, and no one can order an attack on it. The fighter, 1 m away,
    // starts in tick 0 and kills it in tick 2.
    let mut game = orders_match();
    let hero = armed(&mut game, fighter_stats(), Some(meter()));
    let weak = armed(&mut game, combatant(10, 0, 0, 1, 0), Some(meter()));
    let fighter = Orders::hero(&mut game, hero, (0, 0, 0), Team::new(0), 0);
    let doomed = Orders::hero(&mut game, weak, (1, 0, 0), Team::new(1), 1);
    game.step_with(&[(PlayerSlot::new(0), &attack(fighter, doomed))]);
    game.run_until(3);
    assert!(game.try_part::<Dead>(doomed).is_some());
    game.step_with(&[
        (PlayerSlot::new(1), &move_to(doomed, 5, 5)),
        (PlayerSlot::new(0), &attack(fighter, doomed)),
    ]);
    assert_eq!(*game.part::<Position>(doomed), Position::from((1, 0, 0)));
    assert_eq!(game.part::<ActionSlots>(fighter).attack_target(), None);
}

/// 1000 health; reaches nothing and never moves, so only its target changes.
const fn standing() -> Armed {
    combatant(1000, 0, 0, 1, 0)
}

const fn meters(value: i64) -> Scalar {
    Scalar::Decimal(Num::int(value))
}

#[test]
fn a_tower_prefers_creeps_and_defends_its_heroes() {
    let mut game = orders_match();
    let hero = Orders::ai_type(&mut game, &["avatar"], &[], None);
    let creep = Orders::ai_type(&mut game, &["creep"], &[], None);
    let window = ("help_window_ms", Scalar::Int(2000));
    let tower_type = Orders::ai_type(
        &mut game,
        &["structure", "tower"],
        &[window],
        Some(TOWER_AI),
    );
    // The tower strikes 10 within 5 m, a tick into an attack every 3 ticks. The enemy hero
    // strikes the ally hero, 2 m away, a tick into its attack.
    let foe_hero = Orders::ai_type(&mut game, &["avatar"], &[], None);
    let tower_type = arm(&mut game, tower_type, combatant(1000, 5, 1, 3, 10));
    let hero = arm(&mut game, hero, standing());
    let foe_hero = arm(&mut game, foe_hero, combatant(1000, 3, 1, 30, 5));
    let creep = arm(&mut game, creep, standing());
    let tower = Mode::spawn(&mut game, tower_type, (0, 0, 0), Team::new(1), ());
    let ally = Mode::spawn(&mut game, hero, (1, 0, 0), Team::new(1), ());
    let owner = Owner::new(PlayerSlot::new(0));
    let foe = Mode::spawn(&mut game, foe_hero, (3, 0, 0), Team::new(0), owner);
    let enemy_creep = Mode::spawn(&mut game, creep, (4, 0, 0), Team::new(0), ());
    assert_eq!(tower.get(), 0);

    // The tower thinks every 8 ticks, 250 ms at 30 ticks a second being 7.5, in the ticks that
    // leave its id 0: ticks 0 and 8. In tick 0 no ally was struck: it takes the creep 4 m away
    // over the hero 3 m away. The enemy hero strikes the ally in tick 2, so in tick 8 the tower
    // defends it.
    let mut targets = Vec::new();
    for tick in 0..=9 {
        match tick {
            1 => Orders::think(&mut game, &[(PlayerSlot::new(0), &attack(foe, ally))]),
            _ => Orders::think(&mut game, &[]),
        }
        targets.push(game.part::<ActionSlots>(tower).attack_target());
    }
    let expected = [vec![Some(enemy_creep); 8], vec![Some(foe); 2]].concat();
    assert_eq!(targets, expected);
    // Its attacks on the creep start in ticks 0, 3 and 6 and strike a tick later; the next is
    // ready in tick 9, on the hero, and strikes in tick 10.
    assert_eq!(Stats::health(&game, enemy_creep), 970);
    assert_eq!(Stats::health(&game, foe), 1000);
    Orders::think(&mut game, &[]);
    assert_eq!(Stats::health(&game, foe), 990);
}

#[test]
fn a_creep_takes_an_enemy_structure_last_and_keeps_it() {
    let mut game = orders_match();
    let params = [
        ("aggro_range", meters(7)),
        ("help_range", meters(5)),
        ("help_window_ms", Scalar::Int(2000)),
    ];
    Orders::ai_type(&mut game, &["avatar"], &[], None);
    let structure = Orders::ai_type(&mut game, &["structure"], &[], None);
    let creep = Orders::ai_type(&mut game, &["creep"], &params, Some(CREEP_AI));
    let still_creep = Orders::ai_type(&mut game, &["creep"], &[], None);
    for unit_type in [structure, creep, still_creep] {
        arm(&mut game, unit_type, standing());
    }
    let unit = |game: &mut Match, unit_type, team, at| {
        Mode::spawn(game, unit_type, at, Team::new(team), ())
    };
    let first = unit(&mut game, creep, 0, Position::from((0, 0, 0)));
    let second = unit(&mut game, creep, 0, Position::from((0, 0, 20)));
    let tower = unit(&mut game, structure, 1, Position::from((5, 0, 0)));
    let wall = unit(&mut game, structure, 1, Position::from((0, 0, 24)));
    let rival = unit(&mut game, still_creep, 1, Position::from((0, 0, 26)));
    assert_eq!([first, second].map(StableId::get), [0, 1]);

    // The first creep thinks in tick 0: only the tower is in its 7 m, so it takes it. The second
    // thinks in tick 1: the rival creep 6 m away goes before the wall 4 m away.
    Orders::think(&mut game, &[]);
    assert_eq!(game.part::<ActionSlots>(first).attack_target(), Some(tower));
    Orders::think(&mut game, &[]);
    assert_eq!(
        game.part::<ActionSlots>(second).attack_target(),
        Some(rival)
    );
    assert_eq!(game.part::<ActionSlots>(wall).attack_target(), None);

    // A rival creep 3 m from the first creep comes; at its think in tick 8, it keeps the tower.
    unit(&mut game, still_creep, 1, Position::from((3, 0, 0)));
    game.run_until(8);
    Orders::think(&mut game, &[]);
    assert_eq!(game.part::<ActionSlots>(first).attack_target(), Some(tower));
}

#[test]
fn creeps_think_in_turn_and_take_the_targets_their_script_picks() {
    let mut game = orders_match();
    let hero = Orders::ai_type(&mut game, &["avatar"], &[], None);
    Orders::ai_type(&mut game, &["structure"], &[], None);
    let still_creep = Orders::ai_type(&mut game, &["creep"], &[], None);
    let params = [
        ("aggro_range", meters(7)),
        ("help_range", meters(5)),
        ("help_window_ms", Scalar::Int(2000)),
    ];
    let creep = Orders::ai_type(&mut game, &["creep"], &params, Some(CREEP_AI));
    for unit_type in [still_creep, creep] {
        arm(&mut game, unit_type, standing());
    }
    // The foe strikes within 4 m, a tick into its attack.
    let hero = arm(&mut game, hero, combatant(1000, 4, 1, 30, 5));
    let unit = |game: &mut Match, unit_type, team, at| {
        Mode::spawn(game, unit_type, at, Team::new(team), ())
    };
    let owner = Owner::new(PlayerSlot::new(0));
    let foe = Mode::spawn(&mut game, hero, (3, 0, 0), Team::new(1), owner);
    let first = unit(&mut game, creep, 0, Position::from((0, 0, 0)));
    let second = unit(&mut game, creep, 0, Position::from((0, 0, 2)));
    let rival = unit(&mut game, still_creep, 1, Position::from((6, 0, 0)));
    let ally = unit(&mut game, hero, 0, Position::from((0, 0, -1)));
    // Far from everything, with a target it cannot reach.
    let lone = unit(&mut game, creep, 0, Position::from((0, 0, 50)));
    game.part_mut::<ActionSlots>(lone)
        .set_attack_target(Some(rival));
    assert_eq!([first, second, lone].map(StableId::get), [1, 2, 5]);

    // Every 8 ticks, in the ticks that leave their ids: the first creep in ticks 1 and 9, the
    // second in 2 and 10, the lone one in 5. At first each takes the rival creep, 6 m and
    // √40 ≈ 6.32 m away, over the foe, 3 m and √13 ≈ 3.61 m away. The foe strikes their ally in
    // tick 4, so each defends it at its next think. The lone creep's target is beyond its 7 m:
    // with nothing in reach, it goes back to its path.
    let mut targets = Vec::new();
    for tick in 0..=10 {
        match tick {
            3 => Orders::think(&mut game, &[(PlayerSlot::new(0), &attack(foe, ally))]),
            _ => Orders::think(&mut game, &[]),
        }
        targets
            .push([first, second, lone].map(|unit| game.part::<ActionSlots>(unit).attack_target()));
    }
    let expected: Vec<_> = (0..=10)
        .map(|tick| {
            let first = match tick {
                0 => None,
                1..=8 => Some(rival),
                _ => Some(foe),
            };
            let second = match tick {
                0 | 1 => None,
                2..=9 => Some(rival),
                _ => Some(foe),
            };
            let lone = (tick < 5).then_some(rival);
            [first, second, lone]
        })
        .collect();
    assert_eq!(targets, expected);
}

#[test]
fn an_ai_needs_think_and_orders_only_its_own_unit() {
    let mut game = orders_match();
    let ai = AiData {
        ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
        think_ms: 250,
    };
    for source in [
        "fn thinks(ctx, unit) { }",
        "fn on_think(ctx) { }",
        "fn think(ctx, unit) { }",
    ] {
        let unit_type = Orders::ai_type(&mut game, &[], &[], None);
        let script = Units::compile_hooked(&mut game, source).unwrap();
        let error = Orders::load_ai(&mut game, unit_type, &ai, script).unwrap_err();
        assert!(matches!(error, AiError::NoThink), "{source}: {error:?}");
    }

    let meddle = r#"fn on_think(ctx, unit) {
        for ally in ctx.find(unit, unit.pos, 9, "allies") {
            if ally != unit { ctx.order_follow_path(ally); }
        }
    }"#;
    let meddler = Orders::ai_type(&mut game, &[], &[], Some(meddle));
    // Every type loads before a unit spawns, as a match's types do.
    let resetter = Orders::ai_type(
        &mut game,
        &[],
        &[],
        Some("fn on_think(ctx, unit) { ctx.order_reset(unit); }"),
    );
    let striker = Orders::ai_type(
        &mut game,
        &[],
        &[],
        Some(
            r#"fn on_think(ctx, unit) { ctx.order_attack(unit, ctx.find(unit, unit.pos, 20, "enemies")[0]); }"#,
        ),
    );
    for unit_type in [meddler, resetter, striker] {
        arm(&mut game, unit_type, standing());
    }
    let still = armed(&mut game, standing(), None);
    let thinker = Mode::spawn(&mut game, meddler, (0, 0, 0), Team::new(0), ());
    let ally = Mode::spawn(&mut game, still, (1, 0, 0), Team::new(0), ());
    let enemy = Mode::spawn(&mut game, still, (9, 0, 0), Team::new(1), ());
    game.part_mut::<ActionSlots>(ally)
        .set_attack_target(Some(enemy));
    assert_eq!(thinker.get(), 0);

    // It thinks in tick 0: the order for its ally fails the call, which changes nothing.
    game.step_with(&[]);
    let failures = game.non_send::<ScriptFailures>().get();
    assert_eq!(failures.len(), 1);
    assert_eq!(
        (failures[0].id, failures[0].hook),
        (Some(thinker), Hook::OnThink)
    );
    assert!(matches!(
        failures[0].error,
        CallError::Api(ApiError::OtherUnit)
    ));
    assert_eq!(game.part::<ActionSlots>(ally).attack_target(), Some(enemy));

    // A unit the mode spawned holds the place it spawned at, so its reset starts.
    let homing = Mode::spawn(&mut game, resetter, (2, 0, 0), Team::new(0), ());
    // It thinks first in the tick of its id, 3.
    assert_eq!(homing.get(), 3);
    game.run_until(homing.get());
    game.step_with(&[]);
    assert!(game.non_send::<ScriptFailures>().get().is_empty());
    assert!(game.try_part::<Resetting>(homing).is_some());

    // An attack order needs a learned weapon whose filter selects the target. Units 4 and 5 order
    // an attack on the enemy, 9 m off, in ticks 4 and 5: the first's weapon aims at enemies, and
    // attacks; the second's aims only at allies, and its call fails.
    let mut armed = |aim: &str| {
        let weapon = TestWeapon::new(
            Filter::parse(aim, &TagNames::new(&Release::engine_tags())).unwrap(),
            ActionRange::Meters(Num::int(20)),
            Ticks::new(2),
        );
        let weapon = Actions::add_weapon(&mut game, weapon);
        let slots = TimedSlots::new(ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))]));
        Mode::spawn(&mut game, striker, (0, 0, 0), Team::new(0), slots)
    };
    let hostile = armed("enemies");
    let friendly = armed("allies");
    assert_eq!([hostile.get(), friendly.get()], [4, 5]);
    game.run_until(4);
    game.step_with(&[]);
    assert_eq!(
        game.part::<ActionSlots>(hostile).attack_target(),
        Some(enemy)
    );
    assert!(game.non_send::<ScriptFailures>().get().is_empty());
    game.step_with(&[]);
    let failures = game.non_send::<ScriptFailures>().get();
    assert_eq!(failures.len(), 1);
    assert!(matches!(
        (failures[0].id, &failures[0].error),
        (Some(unit), CallError::Api(ApiError::NoAttack)) if unit == friendly
    ));
    assert_eq!(game.part::<ActionSlots>(friendly).attack_target(), None);
    // A reset of another unit fails as the unit's own does not: of a delivery, which holds no
    // spawn place, as of any.
    let delivery = Deliveries::spawn_of(&mut game, homing, still, (2, 0, 1), DeliveryEntity);
    let reset = Units::read(&mut game, "ctx.order_reset(of)", delivery).unwrap_err();
    assert_eq!(reset.kind(), FailureKind::Api(ApiError::OtherUnit));
}

#[test]
fn a_unit_that_finds_the_think_pool_spent_goes_first_next_tick() {
    // A pool of 1500 operations: a spinning call runs its 1000 and fails, which leaves 500; the
    // next is ended past those 500, so its unit stays due.
    let limits = ScriptLimits {
        per_call: NonZeroU64::new(1000).unwrap(),
        player: 1000,
        think: 1500,
        mode: 100_000,
    };
    let mut game = orders_match_with(Paths::default(), limits);
    let spinner = Orders::ai_type(&mut game, &[], &[], None);
    // 1 ms is 0.03 ticks, up to 1: both think in every tick.
    let ai = AiData {
        ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
        think_ms: 1,
    };
    let spin = "fn on_think(ctx, unit) { loop {} }";
    let spin = Units::compile_hooked(&mut game, spin).unwrap();
    Orders::load_ai(&mut game, spinner, &ai, spin).unwrap();
    arm(&mut game, spinner, standing());
    let first = Mode::spawn(&mut game, spinner, (0, 0, 0), Team::new(0), ());
    let second = Mode::spawn(&mut game, spinner, (1, 0, 0), Team::new(0), ());

    // Tick 0: both due since 0; the first runs, the second finds the pool spent and stays due
    // since 0. Tick 1: the second, due since 0, goes before the first, due since 1, and the
    // first stays due. Tick 2: the first again. Each tick one call fails at its limit, and the
    // other waits; none is lost.
    let next = |game: &Match, id| {
        game.try_part::<NextThink>(id)
            .and_then(|next| next.get())
            .map(Tick::get)
    };
    let mut seen = Vec::new();
    for _ in 0..3 {
        game.step_with(&[]);
        let failures = game.non_send::<ScriptFailures>().get();
        let failed: Vec<_> = failures.iter().map(|failure| failure.id).collect();
        seen.push((failed, next(&game, first), next(&game, second)));
    }
    assert_eq!(
        seen,
        [
            (vec![Some(first)], Some(1), Some(0)),
            (vec![Some(second)], Some(1), Some(2)),
            (vec![Some(first)], Some(3), Some(2)),
        ]
    );
    for failure in game.non_send::<ScriptFailures>().get() {
        assert!(matches!(
            failure.error,
            CallError::Script(ScriptError::CallLimit)
        ));
    }
}

#[test]
fn a_walker_goes_back_to_its_path_after_a_chase() {
    let waypoints = [
        Position::from((0, 0, 0)),
        Position::from((4, 0, 0)),
        Position::from((4, 0, 4)),
    ];
    let mut game = orders_match_with_paths(Paths::new([("mid", &waypoints[..])]));
    let chaser = armed(&mut game, combatant(100, 1, 1, 5, 0), Some(meter()));
    let prey = armed(&mut game, dummy(100), None);
    let walker = (
        OnPath::new(PathId::new(0)),
        PathWalker::start(PathEnd::Start, lone()),
    );
    let chaser = Mode::spawn(&mut game, chaser, (0, 0, 0), Team::new(0), walker);
    let prey = Mode::spawn(&mut game, prey, (1, 0, 3), Team::new(1), ());

    // A meter towards the second waypoint, then towards the prey until it is a meter away.
    game.step_with(&[]);
    assert_eq!(*game.part::<Position>(chaser), Position::from((1, 0, 0)));
    game.part_mut::<ActionSlots>(chaser)
        .set_attack_target(Some(prey));
    game.run_until(4);
    assert_eq!(*game.part::<Position>(chaser), Position::from((1, 0, 2)));
    assert_eq!(game.part::<Destination>(chaser).get(), None);

    // The prey goes; in tick 4 the chaser drops it, and in tick 5 walks back to the second
    // waypoint, which it never reached, not on to the third.
    let entity = game.unit_entity(prey);
    game.despawn(entity);
    game.run_until(6);
    assert_eq!(game.part::<ActionSlots>(chaser).attack_target(), None);
    assert_eq!(
        game.part::<Destination>(chaser).get(),
        Some(Position::from((4, 0, 0)))
    );
}

#[test]
fn a_path_walker_that_arrives_short_of_its_waypoint_waits_there() {
    // A path from (0.5, 1.5) to (6.5, 1.5) on 8 by 3 cells of 1 m, walled off by towers of
    // 0.5 m down column 4, each blocking its own cell: a walker with no body, on the first
    // waypoint as it spawns, walks to the nearest cell it reaches, (3.5, 1.5), 3 m on, in
    // ticks 0 to 2. There it waits, with no destination: it never asks for the route again.
    let half = Num::HALF;
    let place = |x: i64, z: i64| {
        Position::new(Vec3::new(Num::int(x) + half, Num::ZERO, Num::int(z) + half))
    };
    let waypoints = [place(0, 1).unwrap(), place(6, 1).unwrap()];
    let mut game = orders_match_with_paths(Paths::new([("mid", &waypoints[..])]));
    let bounds = Bounds::new([Num::int(0), Num::int(0)], [Num::int(8), Num::int(3)]).unwrap();
    let ground = Walker {
        layer: Layer::FIRST,
        radius: Num::ZERO,
    };
    Navigation::load_pathing(
        &mut game,
        Grid::new(Num::ONE, bounds).unwrap(),
        vec![ground],
    );
    let tower = Units::load_next_type(&mut game, &UnitTypeCore::default());
    Units::give_type(&mut game, tower, BodyForm::circle(half).unwrap());
    let walker_type = armed(&mut game, combatant(100, 1, 1, 5, 0), Some(meter()));
    for z in 0..3 {
        Mode::spawn(&mut game, tower, place(4, z).unwrap(), Team::new(0), ());
    }
    let walker = (
        OnPath::new(PathId::new(0)),
        PathWalker::start(PathEnd::Start, lone()),
    );
    let walker = Mode::spawn(&mut game, walker_type, waypoints[0], Team::new(0), walker);
    game.run_until(3);
    assert_eq!(*game.part::<Position>(walker), place(3, 1).unwrap());
    let entity = game.unit_entity(walker);
    let changed = |game: &Match| {
        let unit = game.entity(entity);
        let route = unit.get_ref::<Route>().unwrap().last_changed();
        (route, unit.get_ref::<Destination>().unwrap().last_changed())
    };
    let arrived = changed(&game);
    game.run_until(6);
    assert_eq!(game.part::<Destination>(walker).get(), None);
    assert_eq!(changed(&game), arrived);
}

#[test]
fn a_monster_pulled_past_its_leash_walks_home_ignoring_its_attacker_and_heals() {
    let mut game = orders_match();
    let half = Num::HALF;
    let params = [
        ("leash_range", meters(8)),
        ("home_slack", Scalar::Decimal(half)),
        ("aggro_window_ms", Scalar::Int(5000)),
    ];
    let camp = Orders::ai_type(&mut game, &["camp"], &params, Some(CAMP_AI));
    let home = Position::from((0, 0, 0));
    Units::give_type(&mut game, camp, MoveStep::new(half).unwrap());
    let camp = arm(&mut game, camp, combatant(100, 1, 1, 5, 0));
    let hero = armed(&mut game, fighter_stats(), Some(meter()));
    let monster = Mode::spawn(&mut game, camp, home, Team::new(0), ());
    let hero = Orders::hero(&mut game, hero, (2, 0, 0), Team::new(1), 0);
    assert_eq!(monster.get(), 0);
    let resets = |game: &Match| game.try_part::<Resetting>(monster).is_some();

    // The hero strikes 30 in ticks 2 and 7, then runs from tick 8, a meter a tick. The monster,
    // thinking every 8 ticks, attacks it from tick 8 and follows at half a meter a tick: 4 m out
    // by its think in tick 16, 8 in tick 24, and 12, past its leash of 8, in tick 32: it resets.
    Orders::think(&mut game, &[(PlayerSlot::new(0), &attack(hero, monster))]);
    game.run_until(8);
    Orders::think(&mut game, &[(PlayerSlot::new(0), &move_to(hero, 30, 0))]);
    while game.now().get() < 32 {
        Orders::think(&mut game, &[]);
    }
    assert_eq!(
        game.part::<ActionSlots>(monster).attack_target(),
        Some(hero)
    );
    assert!(!resets(&game));
    Orders::think(&mut game, &[]);
    assert!(resets(&game));
    assert_eq!(game.part::<Destination>(monster).get(), Some(home));
    assert_eq!(Stats::health(&game, monster), 40);

    // It walks the 12 m home in 24 ticks, to tick 55. At its thinks in ticks 40 and 48, 8 and
    // 4 m out, it attacks the hero that struck it, and the reset ignores the order.
    for _ in 33..=55 {
        Orders::think(&mut game, &[]);
        assert!(resets(&game));
        assert_eq!(game.part::<ActionSlots>(monster).attack_target(), None);
    }
    assert_eq!(*game.part::<Position>(monster), home);
    assert_eq!(Stats::health(&game, monster), 40);
    // Home, its pools fill as tick 56 begins, and its think then takes the hero again.
    Orders::think(&mut game, &[]);
    assert!(!resets(&game));
    assert_eq!(Stats::health(&game, monster), 100);
    assert_eq!(
        game.part::<ActionSlots>(monster).attack_target(),
        Some(hero)
    );

    // One that dies while it resets stops resetting, and nothing fills its pools.
    let entity = game.unit_entity(monster);
    let mut unit = game.entity_mut(entity);
    unit.get_mut::<Life>().unwrap().take(Num::int(60));
    unit.insert((Resetting, Dead));
    game.step_with(&[]);
    assert!(!resets(&game));
    assert_eq!(Stats::health(&game, monster), 40);
}

#[test]
fn a_walker_ordered_to_move_leaves_its_path_until_it_follows_it_again() {
    let waypoints = [
        Position::from((0, 0, 0)),
        Position::from((4, 0, 0)),
        Position::from((4, 0, 4)),
    ];
    let mut game = orders_match_with_paths(Paths::new([("mid", &waypoints[..])]));
    // To the post while away from it, and back to the path once there.
    let mover = r#"
fn on_think(ctx, unit) {
    let post = ctx.find(unit, unit.pos, 50, "allies:post")[0];
    if unit.pos.within(post.pos, 0) {
        ctx.order_follow_path(unit);
    } else {
        ctx.order_move(unit, post.pos);
    }
}
"#;
    let walker_type = Orders::ai_type(&mut game, &["walker"], &[], Some(mover));
    let post_type = Orders::ai_type(&mut game, &["post"], &[], None);
    Units::give_type(&mut game, walker_type, meter());
    let walker_type = arm(&mut game, walker_type, dummy(10));
    let post_type = arm(&mut game, post_type, standing());
    let led_type = armed(&mut game, dummy(10), Some(meter()));
    let on_path = || {
        (
            OnPath::new(PathId::new(0)),
            PathWalker::start(PathEnd::Start, lone()),
        )
    };
    let walker = Mode::spawn(&mut game, walker_type, (0, 0, 0), Team::new(0), on_path());
    Mode::spawn(&mut game, post_type, (0, 0, 3), Team::new(0), ());
    assert_eq!(walker.get(), 0);

    // It thinks in tick 0, off the path to the post 3 m away, there in tick 2, and stays: the
    // path no longer draws it to its next waypoint. Its think in tick 8 sends it back to it.
    let mut steps = Vec::new();
    for _ in 0..8 {
        Orders::think(&mut game, &[]);
        steps.push(*game.part::<Position>(walker));
    }
    let post_at = Position::from((0, 0, 3));
    assert_eq!(
        steps,
        [
            Position::from((0, 0, 1)),
            Position::from((0, 0, 2)),
            post_at,
            post_at,
            post_at,
            post_at,
            post_at,
            post_at
        ]
    );
    assert!(game.try_part::<PathWalker>(walker).copied().unwrap().left());
    Orders::think(&mut game, &[]);
    assert!(!game.try_part::<PathWalker>(walker).copied().unwrap().left());
    assert_eq!(
        game.part::<Destination>(walker).get(),
        Some(Position::from((4, 0, 0)))
    );

    // A player's move leaves the path too, as every order applies alike: a walker of player 0
    // on the path, ordered off it, walks there and no longer follows the path.
    let parts = (on_path(), Owner::new(PlayerSlot::new(0)));
    let led = Mode::spawn(&mut game, led_type, (0, 0, 0), Team::new(0), parts);
    game.step_with(&[(PlayerSlot::new(0), &move_to(led, -3, 0))]);
    assert!(game.try_part::<PathWalker>(led).copied().unwrap().left());
    assert_eq!(
        game.part::<Destination>(led).get(),
        Some(Position::from((-3, 0, 0)))
    );
}

#[test]
fn a_walker_follows_its_path_in_its_direction() {
    let waypoints = [
        Position::from((0, 0, 0)),
        Position::from((4, 0, 0)),
        Position::from((4, 0, 4)),
    ];
    let mut game = orders_match_with_paths(Paths::new([("mid", &waypoints[..])]));
    let walker = armed(&mut game, dummy(10), Some(meter()));
    let path_walker = |direction| {
        (
            OnPath::new(PathId::new(0)),
            PathWalker::start(direction, lone()),
        )
    };
    let forward = path_walker(PathEnd::Start);
    let forward = Mode::spawn(&mut game, walker, (0, 0, 0), Team::new(0), forward);
    let backward = path_walker(PathEnd::End);
    let backward = Mode::spawn(&mut game, walker, (4, 0, 4), Team::new(1), backward);

    // Each walks a meter a tick along the waypoints in its direction, then stays.
    let walked: Vec<_> = (0..9)
        .map(|_| {
            game.step_with(&[]);
            [
                *game.part::<Position>(forward),
                *game.part::<Position>(backward),
            ]
        })
        .collect();
    assert_eq!(
        walked,
        [
            [Position::from((1, 0, 0)), Position::from((4, 0, 3))],
            [Position::from((2, 0, 0)), Position::from((4, 0, 2))],
            [Position::from((3, 0, 0)), Position::from((4, 0, 1))],
            [Position::from((4, 0, 0)), Position::from((4, 0, 0))],
            [Position::from((4, 0, 1)), Position::from((3, 0, 0))],
            [Position::from((4, 0, 2)), Position::from((2, 0, 0))],
            [Position::from((4, 0, 3)), Position::from((1, 0, 0))],
            [Position::from((4, 0, 4)), Position::from((0, 0, 0))],
            [Position::from((4, 0, 4)), Position::from((0, 0, 0))],
        ]
    );
}

#[test]
fn every_orders_type_is_state_and_restores() {
    let load = |game: &mut Match| {
        let hero = armed(game, fighter_stats(), Some(meter()));
        let still = armed(game, fighter_stats(), None);
        (hero, still)
    };
    let mut game = orders_match();
    let (hero, still) = load(&mut game);
    let fighter = Orders::hero(&mut game, hero, (0, 0, 0), Team::new(0), 0);
    let still = Mode::spawn(&mut game, still, (9, 0, 0), Team::new(1), ());
    game.step_with(&[(PlayerSlot::new(0), &move_to(fighter, 0, 3))]);
    game.insert_parts(still, Resetting);

    // A restore loads the match's books first: the same types and weapons, in the same order.
    let mut restored = orders_match();
    load(&mut restored);
    Release::restore_into(&game, &mut restored);
    assert_eq!(
        restored.part::<Destination>(fighter).get(),
        Some(Position::from((0, 0, 3)))
    );
    assert!(restored.try_part::<Resetting>(still).is_some());
}

/// The payload of an order of `action` to `units`, in the order given, as a client may send it.
fn order_to(units: &[StableId], action: OrdersAction) -> Vec<u8> {
    let body = Binary::encode(&(units, action));
    Command::encode(&[Command {
        capability: Capability::Orders,
        body: &body,
    }])
}

#[test]
fn a_move_to_a_group_keeps_its_shape_away_from_its_box_and_shares_one_route_search() {
    // Player 0's walkers at (1, 2), (4, 6) and (2, 3), player 1's at (3, 3), and player 0's unit
    // that does not walk at (0, 0), all named by one order, on a pathing grid with no wall. The
    // box of player 0's walkers runs from (1, 2) to (4, 6), centre (2.5, 4). A move to (20, 10)
    // sends each to its offset from the centre added to it: (18.5, 8), (21.5, 12) and (19.5, 9);
    // a move to (3, 4), inside the box, sends each to (3, 4) itself. The others stay. The three
    // plan one search, where three orders of one unit each plan three.
    let half =
        |x: i64, z: i64| Position::new(Vec3::new(Num::HALF * x, Num::ZERO, Num::HALF * z)).unwrap();
    let away = [half(37, 16), half(43, 24), half(39, 18)];
    for (goal, goals, one_each) in [
        ((20, 10), away, false),
        ((3, 4), [Position::from((3, 0, 4)); 3], false),
        ((20, 10), [Position::from((20, 0, 10)); 3], true),
    ] {
        let mut game = orders_match();
        let bounds = Bounds::new([Num::int(-30); 2], [Num::int(30); 2]).unwrap();
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::ZERO,
        };
        let grid = Grid::new(Num::ONE, bounds).unwrap();
        Navigation::load_pathing(&mut game, grid, [walker]);
        let hero = armed(&mut game, dummy(10), Some(meter()));
        let still = armed(&mut game, dummy(10), None);
        let walkers = [
            Position::from((1, 0, 2)),
            Position::from((4, 0, 6)),
            Position::from((2, 0, 3)),
        ]
        .map(|at| Orders::hero(&mut game, hero, at, Team::new(0), 0));
        let other = Orders::hero(&mut game, hero, (3, 0, 3), Team::new(1), 1);
        let still = Orders::hero(&mut game, still, (0, 0, 0), Team::new(0), 0);
        let action = OrdersAction::Move {
            x: Num::int(goal.0),
            z: Num::int(goal.1),
        };
        let payload = if one_each {
            let orders = walkers.map(|unit| Order::one(unit, action));
            Order::payload(&orders)
        } else {
            order_to(&[walkers.as_slice(), &[other, still]].concat(), action)
        };
        game.step_with(&[(PlayerSlot::new(0), &payload)]);
        assert_eq!(
            walkers.map(|id| game.part::<Destination>(id).get()),
            goals.map(Some),
            "{goal:?}"
        );
        assert_eq!(game.part::<Destination>(other).get(), None);
        let searches = game.resource::<RoutePlanner>().searches();
        assert_eq!(searches, if one_each { 3 } else { 1 }, "{goal:?}");
    }
}

#[test]
fn a_list_of_units_out_of_order_or_twice_orders_nothing() {
    let mut game = orders_match();
    let TwoHeroes { first, second } = TwoHeroes::place(&mut game);
    let to = OrdersAction::Move {
        x: Num::int(0),
        z: Num::int(5),
    };
    for units in [vec![], vec![second, first], vec![first, first]] {
        game.step_with(&[(PlayerSlot::new(0), &order_to(&units, to))]);
        assert_eq!(game.part::<Destination>(first).get(), None, "{units:?}");
    }
    game.step_with(&[(PlayerSlot::new(0), &order_to(&[first, second], to))]);
    assert_eq!(
        game.part::<Destination>(first).get(),
        Some(Position::from((0, 0, 5)))
    );
}

#[test]
fn a_stop_ends_what_is_under_way_and_stands_the_unit_off_its_path() {
    // A walker on its path, with an attack target and an attack in its windup, stops: it drops
    // the target, the attack and the destination, and leaves the path; the stop of another
    // player's unit does nothing.
    let waypoints = [Position::from((0, 0, 0)), Position::from((8, 0, 0))];
    let mut game = orders_match_with_paths(Paths::new([("mid", &waypoints[..])]));
    let walker_type = armed(&mut game, fighter_stats(), Some(meter()));
    let target = armed(&mut game, dummy(1000), None);
    let parts = (
        OnPath::new(PathId::new(0)),
        PathWalker::start(PathEnd::Start, lone()),
        Owner::new(PlayerSlot::new(0)),
    );
    let walker = Mode::spawn(&mut game, walker_type, (0, 0, 0), Team::new(0), parts);
    let target = Mode::spawn(&mut game, target, (1, 0, 0), Team::new(1), ());
    game.part_mut::<ActionSlots>(walker)
        .set_attack_target(Some(target));
    game.step_with(&[]);
    assert!(Orders::strikes_at(&game, walker).is_some());
    let stop = order_to(&[walker], OrdersAction::Stop);
    game.step_with(&[(PlayerSlot::new(1), &stop)]);
    assert!(Orders::strikes_at(&game, walker).is_some());
    game.step_with(&[(PlayerSlot::new(0), &stop)]);
    assert_eq!(game.part::<ActionSlots>(walker).attack_target(), None);
    assert_eq!(game.part::<ActionSlots>(walker).in_progress(), None);
    assert_eq!(game.part::<Destination>(walker).get(), None);
    assert!(game.try_part::<PathWalker>(walker).unwrap().left());
    let place = *game.part::<Position>(walker);
    game.step_with(&[]);
    assert_eq!(*game.part::<Position>(walker), place);
    assert_eq!(game.part::<ActionSlots>(walker).attack_target(), None);
    // A second stop finds no destination to drop, and leaves it untouched, as a write would
    // replicate an avatar's.
    let entity = game.unit_entity(walker);
    let changed = |game: &Match| {
        let unit = game.entity(entity);
        unit.get_ref::<Destination>().unwrap().last_changed()
    };
    let before = changed(&game);
    game.step_with(&[(PlayerSlot::new(0), &stop)]);
    assert_eq!(changed(&game), before);
}
