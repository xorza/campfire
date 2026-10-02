use std::num::NonZeroU32;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::change_detection::DetectChanges;
use bevy_ecs::component::Component;
use campfire_content::PackagePath;
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_sim::{Capability, Command, IdAllocator, SimUpdate, TickInput, TypeHash};

use super::*;
use crate::actions::action_book::internals::{self, TestWeapon};
use crate::actions::action_slots::{ActionTarget, InProgress, SlotAim};
use crate::actions::slot_kind::SlotKind;
use crate::capability_set::internals::TestMatch;
use crate::combat::armed::Armed;
use crate::combat::on_death::OnDeath;
use crate::navigation::Navigation;
use crate::navigation::path_walker::PathEnd;
use crate::navigation::walker::Walker;
use crate::scripts::error::ApiError;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::scripts::script_limits::ScriptLimits;
use crate::stats::move_step::MoveStep;
use crate::stats::pool_cost::PoolCost;
use crate::stats::pool_id::PoolId;
use crate::stats::stat_id::StatId;
use crate::units::Units;
use crate::units::filter::Filter;
use crate::units::layer::Layer;
use crate::units::path_id::PathId;
use crate::units::script_view::View;
use crate::units::type_scope::TypeScope;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type_data::UnitTypeData;
use crate::units::unit_types::UnitTypes;
use crate::values::declared_name::DeclaredName;
use crate::values::grid::Grid;
use crate::values::scalar::Scalar;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const ONE: i64 = 1 << 24;
/// The reference 3v3's creep and tower AI as they were when these tests were written: the
/// engine's tests keep their own copies, so a change to the reference mode changes none of them.
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

const CAMP_AI: &str = r"
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
fn combatant(health: i64, range: i64, windup: u64, period: u64, damage: i64) -> Armed {
    Armed::melee(num(health), num(range), windup, period, num(damage)).on_death(OnDeath::Stay)
}

/// 100 health, and 30 damage within 2 m, 2 ticks after the start of an attack every 5 ticks.
fn fighter_stats() -> Armed {
    combatant(100, 2, 2, 5, 30)
}

/// A still target that never attacks and despawns when it dies.
const fn dummy(health: i64) -> Armed {
    Armed::unarmed(Num::from_bits(health << 24))
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
        Match::with_paths(Paths::default())
    }

    fn with_paths(paths: Paths) -> Match {
        let limits = ScriptLimits {
            per_call: 20_000,
            player: 200_000,
            think: 200_000,
            mode: 100_000,
        };
        Match::with(paths, limits)
    }

    fn with(paths: Paths, limits: ScriptLimits) -> Match {
        let scripts = ScriptBudgets::new(limits, 2);
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Navigation,
            Capability::Orders,
        ];
        let TestMatch {
            mut world,
            schedule,
            registry,
        } = TestMatch::new(&declared, RATE, Some(scripts));
        world.insert_resource(paths);
        world.add_schedule(schedule);
        Match { world, registry }
    }

    fn spawn(&mut self, at: Position, parts: impl Bundle) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let unit = self.world.spawn((id, at, parts)).id();
        UnitTags::give_type_tags(&mut self.world, unit);
        id
    }

    /// A hero of `slot` that walks a meter a tick.
    fn hero(&mut self, slot: u32, team: Team, at: Position, combatant: Armed) -> StableId {
        let combatant = self.arm(combatant, team);
        let parts = (
            combatant,
            Navigation::walker(meter()),
            Owner::new(PlayerSlot::new(slot)),
        );
        self.spawn(at, parts)
    }

    fn arm(&mut self, combatant: Armed, team: Team) -> impl Bundle + use<> {
        combatant.bundle(&mut self.world, team, RATE.hz().get())
    }

    /// A unit type of `tags` and `params`, and with the AI script `ai` when it has one, thinking
    /// every 250 ms.
    fn unit_type(
        &mut self,
        tags: &[&str],
        params: &[(&str, Scalar)],
        ai: Option<&str>,
    ) -> UnitType {
        let data = UnitTypeData {
            tags: tags
                .iter()
                .map(|&tag| DeclaredName::new(tag).unwrap())
                .collect(),
            params: params
                .iter()
                .map(|&(name, value)| (DeclaredName::new(name).unwrap(), value))
                .collect(),
        };
        let name = format!("type {}", self.world.non_send::<View>().types_count());
        let unit_type = Units::load_type(&mut self.world, TypeScope::Mode, &name, &data);
        if let Some(source) = ai {
            let ai = AiData {
                ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
                think_ms: 250,
            };
            let script = Units::compile(&mut self.world, source).unwrap();
            Orders::load_ai(&mut self.world, unit_type, &ai, script).unwrap();
        }
        unit_type
    }

    /// Runs a tick, and checks that no script call failed in it.
    fn think(&mut self, inputs: &[(u32, &[u8])]) {
        self.tick(inputs);
        let failures = self.world.non_send::<ScriptFailures>().get();
        assert!(failures.is_empty(), "{failures:?}");
    }

    fn set_target(&mut self, unit: StableId, target: Option<StableId>) {
        let entity = self.world.resource::<EntityIndex>().get(unit).unwrap();
        let mut slots = self.world.get_mut::<ActionSlots>(entity).unwrap();
        slots.set_attack_target(target);
    }

    fn still(&mut self, team: Team, at: Position, combatant: Armed) -> StableId {
        let combatant = self.arm(combatant, team);
        self.spawn(at, combatant)
    }

    fn tick(&mut self, inputs: &[(u32, &[u8])]) {
        let mut tick_inputs = self.world.resource_mut::<TickInputs>();
        for &(slot, payload) in inputs {
            tick_inputs.push(TickInput {
                slot: PlayerSlot::new(slot),
                payload,
            });
        }
        self.world.run_schedule(SimUpdate);
    }

    /// Runs ticks with no inputs until `tick` is the next.
    fn run_until(&mut self, tick: u64) {
        while self.world.resource::<SimTick>().start().get() < tick {
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
        self.get::<Pools>(id)
            .map(|pools| pools.current(PoolId::FIRST).unwrap().round())
    }

    fn slots(&self, id: StableId) -> &ActionSlots {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world.get::<ActionSlots>(entity).unwrap()
    }

    fn target(&self, id: StableId) -> Option<StableId> {
        self.slots(id).attack_target()
    }

    /// When the attack it winds up strikes, if one winds up.
    fn strikes_at(&self, id: StableId) -> Option<Tick> {
        self.slots(id)
            .in_progress()
            .and_then(InProgress::resolves_at)
    }

    /// When its weapon may attack again.
    fn ready_at(&self, id: StableId) -> Tick {
        self.slots(id).slot(0).unwrap().ready_at
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
    let mut commands = Vec::new();
    assert!(Command::read(&payload, |command| commands.push(command)));
    assert_eq!(commands.len(), 2);
    assert!(
        commands
            .iter()
            .all(|command| command.capability == Capability::Orders)
    );
    assert_eq!(commands[1].body, attack.encode());

    // A written payload follows what the buffer holds: 1 command, of capability 5 (`orders`),
    // its 5 body bytes; then the move's payload, the body buffer reused.
    let mut out = vec![9];
    let mut body = Vec::new();
    attack.write_payload(&mut body, &mut out);
    assert_eq!(out, [9, 1, 5, 5, 0xAC, 0x02, 1, 0xAC, 0x02]);
    order.write_payload(&mut body, &mut out);
    assert_eq!(out[9..], Order::payload(&[order]));
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
fn a_slot_order_starts_a_cast_or_a_train_and_no_other_kind() {
    let (mut game, [hero, _]) = two_heroes();
    // Its weapon in slot 0, and a train in slot 1.
    let train = internals::train(&mut game.world, UnitType::new(0));
    let entity = game.world.resource::<EntityIndex>().get(hero).unwrap();
    let mut slots = game.world.get_mut::<ActionSlots>(entity).unwrap();
    slots.grant(SlotKind::new(0), &[train], 1);
    let ordered = |game: &Match| game.slots(hero).in_progress();
    let slot = |slot| {
        Order::payload(&[Order {
            unit: hero,
            action: Action::Slot {
                slot,
                target: ActionTarget::None,
            },
        }])
    };
    // The weapon's slot and a slot it does not have order nothing; the train's orders a train,
    // which no capability here starts.
    let train_order = InProgress::Order {
        aim: SlotAim {
            slot: 1,
            target: ActionTarget::None,
        },
        resolves_at: None,
    };
    for (at, order) in [(0, None), (7, None), (1, Some(train_order))] {
        game.tick(&[(0, &slot(at))]);
        assert_eq!(ordered(&game), order, "slot {at}");
    }
}

#[test]
fn only_its_players_orders_in_the_orders_capability_move_a_unit() {
    let (mut game, [first, second]) = two_heroes();
    game.tick(&[(0, &move_to(first, 0, 5)), (0, &move_to(first, 0, -5))]);
    // The last order in a tick wins.
    assert_eq!(game.position(first), at(0, 0, -1));
    assert_eq!(game.destination(first), Some(at(0, 0, -5)));

    // Past the world's bound, which bounds a match without a map: it clamps to the edge.
    let beyond = Order::payload(&[Order {
        unit: first,
        action: Action::Move {
            x: Position::BOUND + Num::EPSILON,
            z: -num(1),
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
    // Only the order past the bound reaches the first hero, which walks 1 m along z = −1 towards
    // the edge; the second takes only its own player's.
    assert_eq!(game.position(first), at(1, 0, -1));
    assert_eq!(
        game.destination(first),
        Some(Position::new(Vec3::new(Position::BOUND, Num::ZERO, -num(1))).unwrap())
    );
    assert_eq!(
        game.destination(second),
        Some(Position::new(Vec3::new(Position::BOUND, num(2), Num::ZERO)).unwrap())
    );
}

#[test]
fn an_attack_chases_winds_up_and_strikes_each_period() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    let dummy = game.still(Team::new(1), at(6, 0, 0), dummy(100));

    // Ticks 0 to 3 walk 4 m, to 2 m from the dummy. Tick 4 starts an attack: it strikes in tick
    // 6, and the next may start in tick 4 + 5 = 9.
    game.tick(&[(0, &attack(fighter, dummy))]);
    game.run_until(4);
    assert_eq!(game.position(fighter), at(4, 0, 0));
    assert_eq!(game.strikes_at(fighter), None);
    game.run_until(6);
    assert_eq!(game.strikes_at(fighter), Some(Tick::new(6)));
    assert_eq!(game.health(dummy), Some(100));
    game.run_until(7);
    assert_eq!(game.health(dummy), Some(70));
    assert_eq!(game.ready_at(fighter), Tick::new(9));
    assert_eq!(game.destination(fighter), None);

    // Strikes in ticks 11, 16 and 21 take 70 to 40, 10 and 0: the dummy despawns in tick 21.
    game.run_until(21);
    assert_eq!(game.health(dummy), Some(10));
    game.run_until(22);
    assert_eq!(game.health(dummy), None);
    // The next tick finds the target gone, drops it, and stays.
    game.run_until(23);
    assert_eq!(game.target(fighter), None);
    assert_eq!(game.position(fighter), at(4, 0, 0));
}

#[test]
fn a_move_cancels_a_windup_but_not_a_back_swing() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    let dummy = game.still(Team::new(1), at(2, 0, 0), dummy(100));

    // An attack starts in tick 0, in range; a move in tick 1 cancels it before its strike in
    // tick 2, and leaves the fighter ready: the attack ordered in tick 2 starts at once.
    game.tick(&[(0, &attack(fighter, dummy))]);
    assert_eq!(game.strikes_at(fighter), Some(Tick::new(2)));
    game.tick(&[(0, &move_to(fighter, 0, 0))]);
    assert_eq!(game.target(fighter), None);
    assert_eq!(game.strikes_at(fighter), None);
    assert_eq!(game.ready_at(fighter), Tick::new(0));
    game.tick(&[(0, &attack(fighter, dummy))]);
    assert_eq!(game.health(dummy), Some(100));
    assert_eq!(game.strikes_at(fighter), Some(Tick::new(4)));
    game.run_until(5);
    assert_eq!(game.health(dummy), Some(70));
    assert_eq!(game.ready_at(fighter), Tick::new(7));

    // A move in tick 5, after the strike, costs nothing: back to the dummy in tick 6, 2 m
    // away, the next attack starts in tick 7, when ready, and strikes in tick 9.
    game.tick(&[(0, &move_to(fighter, -1, 0))]);
    assert_eq!(game.position(fighter), at(-1, 0, 0));
    game.tick(&[(0, &attack(fighter, dummy))]);
    assert_eq!(game.position(fighter), at(0, 0, 0));
    game.run_until(9);
    assert_eq!(game.strikes_at(fighter), Some(Tick::new(9)));
    assert_eq!(game.health(dummy), Some(70));
    game.run_until(10);
    assert_eq!(game.health(dummy), Some(40));
}

#[test]
fn attack_orders_need_a_living_enemy() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    let ally = game.hero(1, Team::new(0), at(1, 0, 0), fighter_stats());
    let gone = game.still(Team::new(1), at(9, 0, 0), dummy(30));
    let enemy = game.still(Team::new(1), at(1, 0, 0), dummy(100));

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
        assert_eq!(game.target(fighter), None, "{order:?}");
    }
    game.tick(&[(0, &attack(fighter, enemy))]);
    assert_eq!(game.target(fighter), Some(enemy));

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
    assert_eq!(game.target(fighter), None);
}

/// 1000 health; reaches nothing and never moves, so only its target changes.
fn standing() -> Armed {
    combatant(1000, 0, 0, 1, 0)
}

fn meters(value: i64) -> Scalar {
    Scalar::Decimal(num(value))
}

#[test]
fn a_tower_prefers_creeps_and_defends_its_heroes() {
    let mut game = Match::new();
    let hero = game.unit_type(&["avatar"], &[], None);
    let creep = game.unit_type(&["creep"], &[], None);
    let window = ("help_window_ms", Scalar::Int(2000));
    let tower_type = game.unit_type(&["structure", "tower"], &[window], Some(TOWER_AI));
    // The tower strikes 10 within 5 m, a tick into an attack every 3 ticks. The enemy hero
    // strikes the ally hero, 2 m away, a tick into its attack.
    let tower_stats = combatant(1000, 5, 1, 3, 10);
    let tower_stats = game.arm(tower_stats, Team::new(1));
    let tower = game.spawn(at(0, 0, 0), (tower_type, tower_stats));
    let ally_stats = game.arm(standing(), Team::new(1));
    let ally = game.spawn(at(1, 0, 0), (hero, ally_stats));
    let foe_stats = game.arm(combatant(1000, 3, 1, 30, 5), Team::new(0));
    let foe = game.spawn(
        at(3, 0, 0),
        (hero, foe_stats, Owner::new(PlayerSlot::new(0))),
    );
    let creep_stats = game.arm(standing(), Team::new(0));
    let enemy_creep = game.spawn(at(4, 0, 0), (creep, creep_stats));
    assert_eq!(tower.get(), 0);

    // The tower thinks every 8 ticks, 250 ms at 30 ticks a second being 7.5, in the ticks that
    // leave its id 0: ticks 0 and 8. In tick 0 no ally was struck: it takes the creep 4 m away
    // over the hero 3 m away. The enemy hero strikes the ally in tick 2, so in tick 8 the tower
    // defends it.
    let mut targets = Vec::new();
    for tick in 0..=9 {
        match tick {
            1 => game.think(&[(0, &attack(foe, ally))]),
            _ => game.think(&[]),
        }
        targets.push(game.target(tower));
    }
    let expected = [vec![Some(enemy_creep); 8], vec![Some(foe); 2]].concat();
    assert_eq!(targets, expected);
    // Its attacks on the creep start in ticks 0, 3 and 6 and strike a tick later; the next is
    // ready in tick 9, on the hero, and strikes in tick 10.
    assert_eq!(game.health(enemy_creep), Some(970));
    assert_eq!(game.health(foe), Some(1000));
    game.think(&[]);
    assert_eq!(game.health(foe), Some(990));
}

#[test]
fn a_creep_takes_an_enemy_structure_last_and_keeps_it() {
    let mut game = Match::new();
    let params = [
        ("aggro_range", meters(7)),
        ("help_range", meters(5)),
        ("help_window_ms", Scalar::Int(2000)),
    ];
    game.unit_type(&["avatar"], &[], None);
    let structure = game.unit_type(&["structure"], &[], None);
    let creep = game.unit_type(&["creep"], &params, Some(CREEP_AI));
    let still_creep = game.unit_type(&["creep"], &[], None);
    let unit = |game: &mut Match, unit_type, team, at| {
        let stats = game.arm(standing(), Team::new(team));
        game.spawn(at, (unit_type, stats))
    };
    let first = unit(&mut game, creep, 0, at(0, 0, 0));
    let second = unit(&mut game, creep, 0, at(0, 0, 20));
    let tower = unit(&mut game, structure, 1, at(5, 0, 0));
    let wall = unit(&mut game, structure, 1, at(0, 0, 24));
    let rival = unit(&mut game, still_creep, 1, at(0, 0, 26));
    assert_eq!([first, second].map(StableId::get), [0, 1]);

    // The first creep thinks in tick 0: only the tower is in its 7 m, so it takes it. The second
    // thinks in tick 1: the rival creep 6 m away goes before the wall 4 m away.
    game.think(&[]);
    assert_eq!(game.target(first), Some(tower));
    game.think(&[]);
    assert_eq!(game.target(second), Some(rival));
    assert_eq!(game.target(wall), None);

    // A rival creep 3 m from the first creep comes; at its think in tick 8, it keeps the tower.
    unit(&mut game, still_creep, 1, at(3, 0, 0));
    game.run_until(8);
    game.think(&[]);
    assert_eq!(game.target(first), Some(tower));
}

#[test]
fn creeps_think_in_turn_and_take_the_targets_their_script_picks() {
    let mut game = Match::new();
    let hero = game.unit_type(&["avatar"], &[], None);
    game.unit_type(&["structure"], &[], None);
    let still_creep = game.unit_type(&["creep"], &[], None);
    let params = [
        ("aggro_range", meters(7)),
        ("help_range", meters(5)),
        ("help_window_ms", Scalar::Int(2000)),
    ];
    let creep = game.unit_type(&["creep"], &params, Some(CREEP_AI));
    let unit = |game: &mut Match, unit_type, team, at| {
        let stats = game.arm(standing(), Team::new(team));
        game.spawn(at, (unit_type, stats))
    };
    // The foe strikes within 4 m, a tick into its attack.
    let foe_stats = game.arm(combatant(1000, 4, 1, 30, 5), Team::new(1));
    let foe = game.spawn(
        at(3, 0, 0),
        (hero, foe_stats, Owner::new(PlayerSlot::new(0))),
    );
    let first = unit(&mut game, creep, 0, at(0, 0, 0));
    let second = unit(&mut game, creep, 0, at(0, 0, 2));
    let rival = unit(&mut game, still_creep, 1, at(6, 0, 0));
    let ally = unit(&mut game, hero, 0, at(0, 0, -1));
    // Far from everything, with a target it cannot reach.
    let lone = unit(&mut game, creep, 0, at(0, 0, 50));
    game.set_target(lone, Some(rival));
    assert_eq!([first, second, lone].map(StableId::get), [1, 2, 5]);

    // Every 8 ticks, in the ticks that leave their ids: the first creep in ticks 1 and 9, the
    // second in 2 and 10, the lone one in 5. At first each takes the rival creep, 6 m and
    // √40 ≈ 6.32 m away, over the foe, 3 m and √13 ≈ 3.61 m away. The foe strikes their ally in
    // tick 4, so each defends it at its next think. The lone creep's target is beyond its 7 m:
    // with nothing in reach, it goes back to its path.
    let mut targets = Vec::new();
    for tick in 0..=10 {
        match tick {
            3 => game.think(&[(0, &attack(foe, ally))]),
            _ => game.think(&[]),
        }
        targets.push([first, second, lone].map(|unit| game.target(unit)));
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
    let mut game = Match::new();
    let ai = AiData {
        ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
        think_ms: 250,
    };
    for source in [
        "fn thinks(ctx, unit) { }",
        "fn on_think(ctx) { }",
        "fn think(ctx, unit) { }",
    ] {
        let unit_type = game.unit_type(&[], &[], None);
        let script = Units::compile(&mut game.world, source).unwrap();
        let error = Orders::load_ai(&mut game.world, unit_type, &ai, script).unwrap_err();
        assert!(matches!(error, AiError::NoThink), "{source}: {error:?}");
    }

    let meddle = r#"fn on_think(ctx, unit) {
        for ally in ctx.find(unit, unit.pos, 9, "allies") {
            if ally != unit { ctx.order_follow_path(ally); }
        }
    }"#;
    let meddler = game.unit_type(&[], &[], Some(meddle));
    let stats = game.arm(standing(), Team::new(0));
    let thinker = game.spawn(at(0, 0, 0), (meddler, stats));
    let ally = game.still(Team::new(0), at(1, 0, 0), standing());
    let enemy = game.still(Team::new(1), at(9, 0, 0), standing());
    game.set_target(ally, Some(enemy));
    assert_eq!(thinker.get(), 0);

    // It thinks in tick 0: the order for its ally fails the call, which changes nothing.
    game.tick(&[]);
    let failures = game.world.non_send::<ScriptFailures>().get();
    assert_eq!(failures.len(), 1);
    assert_eq!(
        (failures[0].unit, failures[0].hook),
        (Some(thinker), Hook::OnThink)
    );
    assert!(matches!(
        failures[0].error,
        CallError::Api(ApiError::OtherUnit)
    ));
    assert_eq!(game.target(ally), Some(enemy));

    // A unit the mode did not spawn has no spawn place to reset to: the call fails.
    let resetter = game.unit_type(
        &[],
        &[],
        Some("fn on_think(ctx, unit) { ctx.order_reset(unit); }"),
    );
    let stats = game.arm(standing(), Team::new(0));
    let homeless = game.spawn(at(2, 0, 0), (resetter, stats));
    // It thinks first in the tick of its id, 3.
    assert_eq!(homeless.get(), 3);
    game.run_until(homeless.get());
    game.tick(&[]);
    let failures = game.world.non_send::<ScriptFailures>().get();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].unit, Some(homeless));
    assert!(matches!(
        failures[0].error,
        CallError::Api(ApiError::NoSpawnPlace)
    ));
    assert!(game.get::<Resetting>(homeless).is_none());

    // An attack order needs a learned weapon whose filter selects the target. Units 4 and 5 order
    // an attack on the enemy, 9 m off, in ticks 4 and 5: the first's weapon aims at enemies, and
    // attacks; the second's aims only at allies, and its call fails.
    let striker = game.unit_type(
        &[],
        &[],
        Some(r#"fn on_think(ctx, unit) { ctx.order_attack(unit, ctx.find(unit, unit.pos, 20, "enemies")[0]); }"#),
    );
    let mut armed = |aim: &str| {
        let weapon = TestWeapon {
            aim: Filter::parse(aim, &UnitTypes::default()).unwrap(),
            range: Range::Meters(num(20)),
            windup: Ticks::new(2),
            projectile: None,
            rate: StatId::new(0),
            damage: StatId::new(1),
            cost: PoolCost::default(),
            resource_cost: None,
        };
        let weapon = internals::weapon(&mut game.world, weapon);
        let slots = ActionSlots::new([(weapon, SlotKind::new(0), 1)]);
        let stats = game.arm(standing(), Team::new(0));
        let unit = game.spawn(at(0, 0, 0), (striker, stats));
        let entity = game.world.resource::<EntityIndex>().get(unit).unwrap();
        game.world.entity_mut(entity).insert(slots);
        unit
    };
    let hostile = armed("enemies");
    let friendly = armed("allies");
    assert_eq!([hostile.get(), friendly.get()], [4, 5]);
    game.run_until(4);
    game.tick(&[]);
    assert_eq!(game.target(hostile), Some(enemy));
    assert!(game.world.non_send::<ScriptFailures>().get().is_empty());
    game.tick(&[]);
    let failures = game.world.non_send::<ScriptFailures>().get();
    assert_eq!(failures.len(), 1);
    assert!(matches!(
        (failures[0].unit, &failures[0].error),
        (Some(unit), CallError::Api(ApiError::NoAttack)) if unit == friendly
    ));
    assert_eq!(game.target(friendly), None);
}

#[test]
fn a_unit_that_finds_the_think_pool_spent_goes_first_next_tick() {
    // A pool of 1500 operations: a spinning call runs its 1000 and fails, which leaves 500; the
    // next is ended past those 500, so its unit stays due.
    let limits = ScriptLimits {
        per_call: 1000,
        player: 1000,
        think: 1500,
        mode: 100_000,
    };
    let mut game = Match::with(Paths::default(), limits);
    let spinner = game.unit_type(&[], &[], None);
    // 1 ms is 0.03 ticks, up to 1: both think in every tick.
    let ai = AiData {
        ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
        think_ms: 1,
    };
    let spin = "fn on_think(ctx, unit) { loop {} }";
    let spin = Units::compile(&mut game.world, spin).unwrap();
    Orders::load_ai(&mut game.world, spinner, &ai, spin).unwrap();
    let stats = game.arm(standing(), Team::new(0));
    let first = game.spawn(at(0, 0, 0), (spinner, stats));
    let stats = game.arm(standing(), Team::new(0));
    let second = game.spawn(at(1, 0, 0), (spinner, stats));

    // Tick 0: both due since 0; the first runs, the second finds the pool spent and stays due
    // since 0. Tick 1: the second, due since 0, goes before the first, due since 1, and the
    // first stays due. Tick 2: the first again. Each tick one call fails at its limit, and the
    // other waits; none is lost.
    let next = |game: &Match, unit| game.get::<NextThink>(unit).map(|next| next.get().get());
    let mut seen = Vec::new();
    for _ in 0..3 {
        game.tick(&[]);
        let failures = game.world.non_send::<ScriptFailures>().get();
        let failed: Vec<_> = failures.iter().map(|failure| failure.unit).collect();
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
    for failure in game.world.non_send::<ScriptFailures>().get() {
        assert!(matches!(
            failure.error,
            CallError::Script(ScriptError::CallLimit)
        ));
    }
}

#[test]
fn a_walker_goes_back_to_its_path_after_a_chase() {
    let waypoints = [at(0, 0, 0), at(4, 0, 0), at(4, 0, 4)];
    let mut game = Match::with_paths(Paths::new([("mid", &waypoints[..])]));
    let walker = (
        game.arm(combatant(100, 1, 1, 5, 0), Team::new(0)),
        Navigation::walker(meter()),
        OnPath::new(PathId::new(0)),
        PathWalker::start(PathEnd::Start),
    );
    let chaser = game.spawn(at(0, 0, 0), walker);
    let prey = game.still(Team::new(1), at(1, 0, 3), dummy(100));

    // A meter towards the second waypoint, then towards the prey until it is a meter away.
    game.tick(&[]);
    assert_eq!(game.position(chaser), at(1, 0, 0));
    game.set_target(chaser, Some(prey));
    game.run_until(4);
    assert_eq!(game.position(chaser), at(1, 0, 2));
    assert_eq!(game.destination(chaser), None);

    // The prey goes; in tick 4 the chaser drops it, and in tick 5 walks back to the second
    // waypoint, which it never reached, not on to the third.
    let entity = game.world.resource::<EntityIndex>().get(prey).unwrap();
    game.world.despawn(entity);
    game.run_until(6);
    assert_eq!(game.target(chaser), None);
    assert_eq!(game.destination(chaser), Some(at(4, 0, 0)));
}

#[test]
fn a_path_walker_that_arrives_short_of_its_waypoint_waits_there() {
    // A path from (0.5, 1.5) to (6.5, 1.5) on 8 by 3 cells of 1 m, walled off by towers of
    // 0.5 m down column 4, each blocking its own cell: a walker with no body, on the first
    // waypoint as it spawns, walks to the nearest cell it reaches, (3.5, 1.5), 3 m on, in
    // ticks 0 to 2. There it waits, with no destination: it never asks for the route again.
    let half = Num::from_bits(1 << 23);
    let place = |x: i64, z: i64| Position::new(Vec3::new(num(x) + half, Num::ZERO, num(z) + half));
    let waypoints = [place(0, 1).unwrap(), place(6, 1).unwrap()];
    let mut game = Match::with_paths(Paths::new([("mid", &waypoints[..])]));
    let bounds = Bounds::new([num(0), num(0)], [num(8), num(3)]).unwrap();
    let ground = Walker {
        layer: Layer::FIRST,
        radius: Num::ZERO,
    };
    Navigation::load_pathing(
        &mut game.world,
        Grid::new(Num::ONE, bounds).unwrap(),
        vec![ground],
    );
    for z in 0..3 {
        let id = game.world.resource_mut::<IdAllocator>().allocate();
        game.world
            .spawn((id, place(4, z).unwrap(), Body::new(half).unwrap()));
    }
    let walker = (
        game.arm(combatant(100, 1, 1, 5, 0), Team::new(0)),
        Navigation::walker(meter()),
        OnPath::new(PathId::new(0)),
        PathWalker::start(PathEnd::Start),
    );
    let walker = game.spawn(waypoints[0], walker);
    game.run_until(3);
    assert_eq!(game.position(walker), place(3, 1).unwrap());
    let entity = game.world.resource::<EntityIndex>().get(walker).unwrap();
    let changed = |game: &Match| {
        let unit = game.world.entity(entity);
        let route = unit.get_ref::<Route>().unwrap().last_changed();
        (route, unit.get_ref::<Destination>().unwrap().last_changed())
    };
    let arrived = changed(&game);
    game.run_until(6);
    assert_eq!(game.destination(walker), None);
    assert_eq!(changed(&game), arrived);
}

#[test]
fn a_monster_pulled_past_its_leash_walks_home_ignoring_its_attacker_and_heals() {
    let mut game = Match::new();
    let half = Num::from_bits(1 << 23);
    let params = [
        ("leash_range", meters(8)),
        ("home_slack", Scalar::Decimal(half)),
        ("aggro_window_ms", Scalar::Int(5000)),
    ];
    let camp = game.unit_type(&["camp"], &params, Some(CAMP_AI));
    let home = at(0, 0, 0);
    let arms = game.arm(combatant(100, 1, 1, 5, 0), Team::new(0));
    let parts = (
        camp,
        arms,
        Navigation::walker(MoveStep::new(half).unwrap()),
        SpawnPoint::new(home),
    );
    let monster = game.spawn(home, parts);
    let hero = game.hero(0, Team::new(1), at(2, 0, 0), fighter_stats());
    assert_eq!(monster.get(), 0);
    let resets = |game: &Match| game.get::<Resetting>(monster).is_some();

    // The hero strikes 30 in ticks 2 and 7, then runs from tick 8, a meter a tick. The monster,
    // thinking every 8 ticks, attacks it from tick 8 and follows at half a meter a tick: 4 m out
    // by its think in tick 16, 8 in tick 24, and 12, past its leash of 8, in tick 32: it resets.
    game.think(&[(0, &attack(hero, monster))]);
    game.run_until(8);
    game.think(&[(0, &move_to(hero, 30, 0))]);
    while game.world.resource::<SimTick>().start().get() < 32 {
        game.think(&[]);
    }
    assert_eq!(game.target(monster), Some(hero));
    assert!(!resets(&game));
    game.think(&[]);
    assert!(resets(&game));
    assert_eq!(game.destination(monster), Some(home));
    assert_eq!(game.health(monster), Some(40));

    // It walks the 12 m home in 24 ticks, to tick 55. At its thinks in ticks 40 and 48, 8 and
    // 4 m out, it attacks the hero that struck it, and the reset ignores the order.
    for _ in 33..=55 {
        game.think(&[]);
        assert!(resets(&game));
        assert_eq!(game.target(monster), None);
    }
    assert_eq!(game.position(monster), home);
    assert_eq!(game.health(monster), Some(40));
    // Home, its pools fill as tick 56 begins, and its think then takes the hero again.
    game.think(&[]);
    assert!(!resets(&game));
    assert_eq!(game.health(monster), Some(100));
    assert_eq!(game.target(monster), Some(hero));

    // One that dies while it resets stops resetting, and nothing fills its pools.
    let entity = game.world.resource::<EntityIndex>().get(monster).unwrap();
    let mut unit = game.world.entity_mut(entity);
    unit.get_mut::<Pools>()
        .unwrap()
        .take(PoolId::FIRST, num(60));
    unit.insert((Resetting, Dead));
    game.tick(&[]);
    assert!(!resets(&game));
    assert_eq!(game.health(monster), Some(40));
}

#[test]
fn a_walker_ordered_to_move_leaves_its_path_until_it_follows_it_again() {
    let waypoints = [at(0, 0, 0), at(4, 0, 0), at(4, 0, 4)];
    let mut game = Match::with_paths(Paths::new([("mid", &waypoints[..])]));
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
    let walker_type = game.unit_type(&["walker"], &[], Some(mover));
    let post_type = game.unit_type(&["post"], &[], None);
    let parts = (
        walker_type,
        game.arm(dummy(10), Team::new(0)),
        Navigation::walker(meter()),
        OnPath::new(PathId::new(0)),
        PathWalker::start(PathEnd::Start),
    );
    let walker = game.spawn(at(0, 0, 0), parts);
    let post = game.arm(standing(), Team::new(0));
    game.spawn(at(0, 0, 3), (post_type, post));
    assert_eq!(walker.get(), 0);

    // It thinks in tick 0, off the path to the post 3 m away, there in tick 2, and stays: the
    // path no longer draws it to its next waypoint. Its think in tick 8 sends it back to it.
    let mut steps = Vec::new();
    for _ in 0..8 {
        game.think(&[]);
        steps.push(game.position(walker));
    }
    let post_at = at(0, 0, 3);
    assert_eq!(
        steps,
        [
            at(0, 0, 1),
            at(0, 0, 2),
            post_at,
            post_at,
            post_at,
            post_at,
            post_at,
            post_at
        ]
    );
    assert!(game.get::<PathWalker>(walker).unwrap().left());
    game.think(&[]);
    assert!(!game.get::<PathWalker>(walker).unwrap().left());
    assert_eq!(game.destination(walker), Some(at(4, 0, 0)));

    // A player's move leaves the path too, as every order applies alike: a walker of player 0
    // on the path, ordered off it, walks there and no longer follows the path.
    let parts = (
        game.arm(dummy(10), Team::new(0)),
        Navigation::walker(meter()),
        OnPath::new(PathId::new(0)),
        PathWalker::start(PathEnd::Start),
        Owner::new(PlayerSlot::new(0)),
    );
    let led = game.spawn(at(0, 0, 0), parts);
    game.tick(&[(0, &move_to(led, -3, 0))]);
    assert!(game.get::<PathWalker>(led).unwrap().left());
    assert_eq!(game.destination(led), Some(at(-3, 0, 0)));
}

#[test]
fn a_walker_follows_its_path_in_its_direction() {
    let waypoints = [at(0, 0, 0), at(4, 0, 0), at(4, 0, 4)];
    let mut game = Match::with_paths(Paths::new([("mid", &waypoints[..])]));
    let path_walker = |game: &mut Match, team, direction| {
        (
            game.arm(dummy(10), team),
            Navigation::walker(meter()),
            OnPath::new(PathId::new(0)),
            PathWalker::start(direction),
        )
    };
    let forward = path_walker(&mut game, Team::new(0), PathEnd::Start);
    let forward = game.spawn(at(0, 0, 0), forward);
    let backward = path_walker(&mut game, Team::new(1), PathEnd::End);
    let backward = game.spawn(at(4, 0, 4), backward);

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
fn every_orders_type_is_state_and_restores() {
    let mut game = Match::new();
    let fighter = game.hero(0, Team::new(0), at(0, 0, 0), fighter_stats());
    let still = game.still(Team::new(1), at(9, 0, 0), fighter_stats());
    game.tick(&[(0, &move_to(fighter, 0, 3))]);
    let entity = game.world.resource::<EntityIndex>().get(still).unwrap();
    game.world.entity_mut(entity).insert(Resetting);

    let registry = &game.registry;
    let mut per_type = Vec::new();
    let hash = registry.hash_by_type(&game.world, &mut per_type);
    let names: Vec<_> = per_type.iter().map(|TypeHash { name, .. }| *name).collect();
    for name in [
        "units.owner",
        "units.team",
        "orders.next_think",
        "orders.resetting",
    ] {
        assert!(names.contains(&name), "{name}");
    }

    let mut snapshot = Vec::new();
    registry.snapshot(&game.world, &mut snapshot);
    // A restore loads the match's books first: the same weapons, in the same order.
    let mut restored = Match::new();
    let _weapon = restored.arm(fighter_stats(), Team::new(0));
    let _weapon = restored.arm(fighter_stats(), Team::new(1));
    registry.restore(&snapshot, &mut restored.world).unwrap();
    assert_eq!(registry.hash(&restored.world), hash);
    assert_eq!(restored.destination(fighter), Some(at(0, 0, 3)));
    assert!(restored.get::<Resetting>(still).is_some());
}
