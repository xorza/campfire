use std::collections::BTreeMap;
use std::num::{NonZeroU8, NonZeroU32};

use bevy_ecs::bundle::Bundle;
use bevy_ecs::component::Component;
use bevy_ecs::world::Mut;
use campfire_content::PackagePath;
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_script::{NumError, ScriptError};
use campfire_sim::{Capability, EntityIndex, IdAllocator, SimUpdate, StateHash};

use super::*;
use crate::actions::Actions;
use crate::actions::action_data::ActionData;
use crate::actions::action_data::{CostTarget, Range, RangeField, Targeting};
use crate::actions::action_data_field::ActionDataField;
use crate::actions::action_slots::{ActionSlot, InProgress};
use crate::actions::delivery_data::DeliveryData;
use crate::actions::effect_data::EffectTo;
use crate::actions::effect_data::{EffectData, Effecting};
use crate::actions::error::{ActionError, ActionField};
use crate::actions::slot_kind::SlotKind;
use crate::areas::area::Area;
use crate::areas::area_data::{AreaData, AreaInside};
use crate::capability_set::internals::TestMatch;
use crate::combat::armed::Armed;
use crate::combat::assist_window::AssistWindow;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_kind::DamageKind;
use crate::combat::on_death::OnDeath;
use crate::combat::pass_queue::PassQueue;
use crate::combat::recent_attackers::RecentAttackers;
use crate::orders::Orders;
use crate::orders::ai_data::AiData;
use crate::players::resource_id::ResourceId;
use crate::projectiles::projectile::Projectile;
use crate::projectiles::projectile_data::ProjectileData;
use crate::scripts::error::ApiError;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::{ScriptFailure, ScriptFailures};
use crate::scripts::script_limits::ScriptLimits;
use crate::stats;
use crate::stats::Stats;
use crate::stats::level::Level;
use crate::stats::modifier_book::{Applier, ModifierBook};
use crate::stats::modifier_data::{ModifierData, Reapply};
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifiers::Modifiers;
use crate::stats::pool_id::PoolId;
use crate::stats::stat::Stat;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_change::StatChange;
use crate::stats::stat_graph::StatGraph;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::stats::stats_data::{StatValue, StatsData};
use crate::stats::unit_stats::UnitStats;
use crate::units::Units;
use crate::units::body::Body;
use crate::units::script_view::View;
use crate::units::tag_data::TagData;
use crate::units::type_scope::TypeScope;
use crate::units::unit_type_data::UnitTypeData;
use crate::values::declared_name::DeclaredName;
use crate::values::filter_data::FilterData;
use crate::values::number::Number;
use crate::values::number::ParamRef;
use crate::values::param::Param;
use crate::values::param::Scaling;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;

/// The MOBA's 30 ticks a second.
const RATE: TickRate = TickRate::new(NonZeroU32::new(30).unwrap());

const LIMITS: ScriptLimits = ScriptLimits {
    per_call: 10_000,
    player: 100_000,
    think: 100_000,
    mode: 100_000,
};
/// Lash Out as the reference Husk had it when these tests were written: the engine's tests keep
/// their own copy, so a balance change to the reference hero changes none of them.
const LASH_OUT: &str = r#"
fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {
        ctx.damage(unit, ctx.p.damage, "magic");
    }
}

fn on_damage_taken(ctx, m, d) {
    if d.attack {
        ctx.reduce_cooldown(m.carrier, "lash_out", ctx.p.cooldown_cut_ms);
    }
}
"#;

fn int(value: i64) -> Number {
    Number::Value(Scalar::Int(value))
}

fn param(name: &str) -> Number {
    Number::Param(ParamRef {
        param: DeclaredName::new(name).unwrap(),
    })
}

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

/// Halves of a meter.
fn halves(value: i64) -> Num {
    Num::from_bits(value << (Num::FRAC_BITS - 1))
}

fn at(x: Num, y: Num, z: Num) -> Position {
    Position::new(Vec3::new(x, y, z)).unwrap()
}

/// A unit that strikes for `damage` within 2 m, as its windup of no ticks ends, every 5 ticks.
fn striker(damage: i64) -> Armed {
    Armed::melee(num(500), num(2), 0, 5, num(damage)).on_death(OnDeath::Stay)
}

/// The match's pools, by name in order: the life pool first, as it is until a mode binds one.
const POOLS: [&str; 3] = ["health", "mana", "rage"];
const MANA: PoolId = PoolId::new(1).unwrap();
const RAGE: PoolId = PoolId::new(2).unwrap();

/// A cost of `amount` in pool `name`.
fn cost(name: &str, amount: Number) -> BTreeMap<DeclaredName, Ranked<Number>> {
    BTreeMap::from([(DeclaredName::new(name).unwrap(), Ranked::One(amount))])
}

/// Husk's Lash Out as its data declares it: no target, a cooldown of 10 s down to 6 s, 35 of the
/// caster's mana, and damage within 3.5 m of 75 to 175, plus half the caster's ability power.
fn lash_out() -> ActionData {
    let scalars = |values: &[i64]| values.iter().map(|&value| Scalar::Int(value)).collect();
    ActionData {
        kind: ActionKind::Cast,
        script: Some(PackagePath::parse("scripts/lash_out.rhai").unwrap()),
        targeting: Targeting::None,
        range: None,
        cooldown_ms: Some(Ranked::PerRank(
            [10_000, 9000, 8000, 7000, 6000].map(int).to_vec(),
        )),
        cost: cost("mana", int(35)),
        windup_ms: None,
        clamp_to_range: false,
        toggle: None,
        channel: None,
        hold: None,
        charges: None,
        charge: None,
        passive_modifier: None,
        passive_while_ready: false,
        delivery: None,
        rate: None,
        damage: None,
        damage_kind: None,
        unit_type: None,
        projectile_state: BTreeMap::new(),
        on_resolve: Vec::new(),
        on_hit: Vec::new(),
        on_end: Vec::new(),
        params: BTreeMap::from([
            (
                DeclaredName::new("radius").unwrap(),
                Param::Ranked(Ranked::One(Scalar::Decimal(halves(7)))),
            ),
            (
                DeclaredName::new("damage").unwrap(),
                Param::Scaling(Scaling {
                    base: Ranked::PerRank(scalars(&[75, 100, 125, 150, 175])),
                    per_level: Num::ZERO,
                    bonus: BTreeMap::new(),
                    ratios: [(Stat::named("ability_power").unwrap(), halves(1))].into(),
                }),
            ),
            (
                DeclaredName::new("cooldown_cut_ms").unwrap(),
                Param::Ranked(Ranked::One(Scalar::Int(500))),
            ),
        ]),
    }
}

/// A unit-targeted ability: `damage` true damage to an enemy within 5 m, every second, for 10
/// mana and 4 rage.
fn strike() -> ActionData {
    ActionData {
        kind: ActionKind::Cast,
        script: Some(PackagePath::parse("strike.rhai").unwrap()),
        targeting: Targeting::Unit(FilterData::parse("enemies").unwrap()),
        range: Some(Ranked::One(RangeField::Range(Range::Meters(num(5))))),
        cooldown_ms: Some(Ranked::One(int(1001))),
        cost: BTreeMap::from([
            (DeclaredName::new("mana").unwrap(), Ranked::One(int(10))),
            (DeclaredName::new("rage").unwrap(), Ranked::One(int(4))),
        ]),
        windup_ms: None,
        clamp_to_range: false,
        toggle: None,
        channel: None,
        hold: None,
        charges: None,
        charge: None,
        passive_modifier: None,
        passive_while_ready: false,
        delivery: None,
        rate: None,
        damage: None,
        damage_kind: None,
        unit_type: None,
        projectile_state: BTreeMap::new(),
        on_resolve: Vec::new(),
        on_hit: Vec::new(),
        on_end: Vec::new(),
        params: BTreeMap::from([(
            DeclaredName::new("damage").unwrap(),
            Param::Ranked(Ranked::One(Scalar::Int(50))),
        )]),
    }
}

#[test]
fn the_field_table_names_the_first_field_a_kind_misuses() {
    // Strike is a cast: its script, cooldown, cost and params are a cast's to take.
    let mut action = strike();
    assert_eq!(ActionDataField::misused(&action, ActionKind::Cast), None);
    // As an attack, its script comes first of what an attack refuses; as a train, also its
    // script, before the range and the unit type it lacks.
    assert_eq!(
        ActionDataField::misused(&action, ActionKind::Attack),
        Some(ActionDataField::Script)
    );
    assert_eq!(
        ActionDataField::misused(&action, ActionKind::Train),
        Some(ActionDataField::Script)
    );
    // With no cast fields, an attack lacks its weapon's rate, then a cast refuses the rate.
    action.script = None;
    action.cooldown_ms = None;
    action.params = BTreeMap::new();
    assert_eq!(
        ActionDataField::misused(&action, ActionKind::Attack),
        Some(ActionDataField::Rate)
    );
    action.rate = Some(Stat::named("armor").unwrap());
    action.damage = Some(Stat::named("attack_damage").unwrap());
    action.damage_kind = Some(DeclaredName::new("physical").unwrap());
    assert_eq!(ActionDataField::misused(&action, ActionKind::Attack), None);
    assert_eq!(
        ActionDataField::misused(&action, ActionKind::Cast),
        Some(ActionDataField::Rate)
    );
    // A train refuses its range first, then needs its unit type.
    assert_eq!(
        ActionDataField::misused(&action, ActionKind::Train),
        Some(ActionDataField::Range)
    );
    let train = ActionData {
        targeting: Targeting::None,
        range: None,
        rate: None,
        damage: None,
        damage_kind: None,
        ..action
    };
    assert_eq!(
        ActionDataField::misused(&train, ActionKind::Train),
        Some(ActionDataField::UnitType)
    );
}

const STRIKE: &str =
    r#"fn on_resolve(ctx, caster, target) { ctx.damage(target, ctx.p.damage, "true"); }"#;

#[derive(Debug)]
struct Match {
    world: World,
    registry: StateRegistry,
}

impl Match {
    fn new() -> Match {
        Match::with(
            LIMITS,
            &[Capability::Stats, Capability::Combat, Capability::Abilities],
        )
    }

    /// A match of two players of `declared`, whose scripts run within `limits`, and who hold
    /// gold, as a mode would keep it.
    fn with(limits: ScriptLimits, declared: &[Capability]) -> Match {
        let TestMatch {
            mut world,
            schedule,
            registry,
        } = TestMatch::new(declared, RATE, Some(ScriptBudgets::new(limits, 2)));
        // The damage kinds a mode would declare: the reference MOBA's.
        Units::name_kinds(&world, &["physical", "magic", "true"], &POOLS, &["gold"]);
        world.add_schedule(schedule);
        world.insert_resource(PlayerResources::new(2, 1));
        Match { world, registry }
    }

    /// Gives the match a stat book of the stats the scaling params name, with no unit type.
    fn load_stats(&mut self) {
        let rules = scaling_stats().map(|stat| (stat, StatRule::default()));
        stats::loads::load_stats(&mut self.world, &BTreeMap::from(rules));
    }

    /// Loads `data` as the action `name` of package 0, of 5 ranks, with its script `source`.
    fn load(&mut self, name: &str, data: &ActionData, source: &str) -> ActionId {
        let script = Units::compile(&mut self.world, source).unwrap();
        Actions::load(&mut self.world, 0, name, data, Some(script), 5).unwrap()
    }

    /// A unit of `team` with 500 health that stays when it dies, and `parts`, with no slots
    /// unless they hold some.
    fn spawn(&mut self, team: u8, at: Position, parts: impl Bundle) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let combat = (
            Team::new(team),
            Pools::life(num(500)),
            OnDeath::Stay,
            RecentAttackers::default(),
        );
        let unit = self.world.spawn((id, at, combat, parts)).id();
        self.world
            .entity_mut(unit)
            .insert_if_new(ActionSlots::new([]));
        UnitTags::give_type_tags(&mut self.world, unit);
        id
    }

    /// A unit of `team` armed as `armed` that attacks `target`.
    fn attacker(&mut self, team: u8, at: Position, armed: Armed, target: StableId) -> StableId {
        let id = self.world.resource_mut::<IdAllocator>().allocate();
        let armed = armed.bundle(&mut self.world, Team::new(team), RATE.hz().get());
        let unit = self.world.spawn((id, at, armed)).id();
        UnitTags::give_type_tags(&mut self.world, unit);
        let mut slots = self.world.get_mut::<ActionSlots>(unit).unwrap();
        slots.set_attack_target(Some(target));
        id
    }

    /// Player 0's caster at the origin, on team 0, with `ability` at `rank`, 100 mana and 20
    /// rage.
    fn caster(&mut self, ability: ActionId, rank: u8) -> StableId {
        let caster = self.spawn(
            0,
            at(Num::ZERO, Num::ZERO, Num::ZERO),
            (
                Owner::new(PlayerSlot::new(0)),
                ActionSlots::new([(ability, SlotKind::new(0), rank)]),
            ),
        );
        self.give_pools(caster, 100, 20);
        caster
    }

    /// Gives unit `id` full pools: its 500 health, `mana` and `rage`.
    fn give_pools(&mut self, id: StableId, mana: i64, rage: i64) {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        let pools = [(PoolId::FIRST, 500), (MANA, mana), (RAGE, rage)];
        let pools = Pools::new(pools.map(|(pool, max)| (pool, num(max)))).unwrap();
        self.world.entity_mut(entity).insert(pools);
    }

    fn cast(&mut self, unit: StableId, target: ActionTarget) {
        self.casts(&[(unit, target)]);
    }

    /// Runs a tick in which each unit casts its first slot's ability at its target, as an order
    /// would make it.
    fn casts(&mut self, casts: &[(StableId, ActionTarget)]) {
        for &(unit, target) in casts {
            let entity = self.world.resource::<EntityIndex>().get(unit).unwrap();
            let mut slots = self.world.get_mut::<ActionSlots>(entity).unwrap();
            slots.order(0, ActionKind::Cast, target);
        }
        self.world.run_schedule(SimUpdate);
    }

    fn run_until(&mut self, tick: u64) {
        while self.world.resource::<SimTick>().start().get() < tick {
            self.world.run_schedule(SimUpdate);
        }
    }

    fn get<C: Component + Copy>(&self, id: StableId) -> C {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        *self.world.entity(entity).get::<C>().unwrap()
    }

    fn get_ref<C: Component>(&self, id: StableId) -> &C {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world.entity(entity).get::<C>().unwrap()
    }

    fn health(&self, id: StableId) -> i64 {
        self.pool_of(id, PoolId::FIRST)
    }

    fn pool(&self, id: StableId) -> i64 {
        self.pool_of(id, MANA)
    }

    fn pool_of(&self, id: StableId, pool: PoolId) -> i64 {
        self.get::<Pools>(id).current(pool).unwrap().round()
    }

    fn slot(&self, id: StableId) -> ActionSlot {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world
            .entity(entity)
            .get::<ActionSlots>()
            .unwrap()
            .slot(0)
            .unwrap()
    }

    /// Gives unit `id` tags that block `blocks`, as its modifiers would.
    fn set_blocks(&mut self, id: StableId, blocks: &[Block]) {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        self.world
            .entity_mut(entity)
            .insert(UnitTags::blocking(blocks));
    }

    fn casting(&self, id: StableId) -> Option<InProgress> {
        let entity = self.world.resource::<EntityIndex>().get(id).unwrap();
        let slots = self.world.get::<ActionSlots>(entity).unwrap();
        slots
            .in_progress()
            .filter(|underway| underway.kind == ActionKind::Cast)
    }

    fn failures(&self) -> &[ScriptFailure] {
        self.world.non_send::<ScriptFailures>().get()
    }
}

#[test]
fn damage_of_a_kind_the_mode_does_not_declare_fails_the_cast() {
    let mut game = Match::new();
    game.load_stats();
    let fire = game.load(
        "fire",
        &lash_out(),
        &LASH_OUT.replace(r#""magic""#, r#""fire""#),
    );
    let husk = game.caster(fire, 2);
    let near = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), ());
    game.cast(husk, ActionTarget::None);
    // The call fails, so the cast applies nothing: no damage, no cost.
    assert_eq!((game.health(near), game.pool(husk)), (500, 100));
    let errors: Vec<_> = game
        .failures()
        .iter()
        .map(|failure| &failure.error)
        .collect();
    assert!(
        matches!(errors[..], [CallError::Api(ApiError::UnknownDamageKind)]),
        "{errors:?}"
    );
}

#[test]
fn lash_out_hits_every_enemy_within_its_radius_exactly() {
    let mut game = Match::new();
    game.load_stats();
    // Strike's one param name comes first in the frame, so Lash Out's three follow from the
    // second.
    game.load("strike", &strike(), STRIKE);
    let lash_out = game.load("lash_out", &lash_out(), LASH_OUT);
    let husk = game.caster(lash_out, 2);
    let near = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), ());
    // At exactly 3.5 m: within the radius.
    let edge = game.spawn(1, at(Num::ZERO, Num::ZERO, halves(7)), ());
    let beyond = game.spawn(1, at(num(4), Num::ZERO, Num::ZERO), ());
    // Up at y = 9, 2 m away on the ground plane.
    let high = game.spawn(1, at(Num::ZERO, num(9), num(2)), ());
    let ally = game.spawn(0, at(num(1), Num::ZERO, Num::ZERO), ());
    let dead = game.spawn(1, at(num(1), Num::ZERO, num(1)), ());
    let entity = game.world.resource::<EntityIndex>().get(dead).unwrap();
    game.world.entity_mut(entity).insert(Dead);

    // Rank 2 deals 100, and 0.5 × 0 ability power: 500 → 400 for the three enemies in reach.
    // It costs 35 of 100, and its 9000 ms cooldown is 9 × 30 = 270 ticks.
    game.cast(husk, ActionTarget::None);
    let healths =
        |game: &Match| [near, edge, beyond, high, ally, dead].map(|unit| game.health(unit));
    assert_eq!(healths(&game), [400, 400, 500, 400, 500, 500]);
    assert_eq!(game.pool(husk), 65);
    assert_eq!(game.slot(husk).ready_at, Tick::new(270));
    assert!(game.failures().is_empty());

    // On cooldown until tick 270: a cast in tick 1 does nothing.
    game.cast(husk, ActionTarget::None);
    assert_eq!(healths(&game), [400, 400, 500, 400, 500, 500]);
    assert_eq!(game.pool(husk), 65);
    // An ability that takes no target ignores the one its order names: the cast at the ally hits
    // the same three enemies.
    game.run_until(270);
    game.cast(husk, ActionTarget::Unit(ally));
    assert_eq!(healths(&game), [300, 300, 500, 300, 500, 500]);
    assert_eq!(game.pool(husk), 30);
    // 30 left cannot pay 35.
    game.run_until(540);
    game.cast(husk, ActionTarget::None);
    assert_eq!(healths(&game), [300, 300, 500, 300, 500, 500]);
    assert_eq!(game.pool(husk), 30);
}

#[test]
fn ai_load_does_not_spend_what_a_cast_needs() {
    // The one test of how abilities and `orders` meet: AI's calls and a cast's draw from pools of
    // their own.
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
    ];
    let mut game = Match::with(LIMITS, &declared);
    let strike = game.load("strike", &strike(), STRIKE);
    let caster = game.caster(strike, 1);
    let enemy = game.spawn(1, at(num(5), Num::ZERO, Num::ZERO), ());
    // Eleven units whose AI spins, all due in every tick: ten calls fail at the 10 000 limit and
    // spend the 100 000 of the think pool, and the eleventh finds it spent.
    let spinner = Units::load_type(
        &mut game.world,
        TypeScope::Mode,
        "spinner",
        &UnitTypeData::default(),
    );
    let ai = AiData {
        ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
        think_ms: 1,
    };
    let spin = "fn on_think(ctx, unit) { loop {} }";
    let spin = Units::compile(&mut game.world, spin).unwrap();
    Orders::load_ai(&mut game.world, spinner, &ai, spin).unwrap();
    for z in 0..11 {
        game.spawn(2, at(Num::ZERO, Num::ZERO, num(20 + z)), spinner);
    }

    // The cast in the same tick draws from its player's pool, whole: 500 → 450, 100 → 90.
    game.cast(caster, ActionTarget::Unit(enemy));
    assert_eq!(game.health(enemy), 450);
    assert_eq!(game.pool(caster), 90);
    let failures = game.failures();
    assert_eq!(failures.len(), 10);
    assert!(failures.iter().all(|failure| failure.hook == Hook::OnThink));
    let mut budgets = game.world.resource_mut::<ScriptBudgets>();
    assert_eq!(budgets.get_mut(Pool::Think).left(), 0);
}

#[test]
fn a_cast_passes_its_checks_or_does_nothing() {
    let mut game = Match::new();
    let strike = game.load("strike", &strike(), STRIKE);
    let caster = game.caster(strike, 1);
    let mut caster_at = |x: i64, rank: u8, mana: i64, rage: i64| {
        let unit = game.spawn(
            0,
            at(num(x), Num::ZERO, Num::ZERO),
            (
                Owner::new(PlayerSlot::new(0)),
                ActionSlots::new([(strike, SlotKind::new(0), rank)]),
            ),
        );
        game.give_pools(unit, mana, rage);
        unit
    };
    let unlearned = caster_at(1, 0, 100, 20);
    let poor = caster_at(2, 1, 5, 20);
    let calm = caster_at(2, 1, 100, 3);
    let spent = caster_at(2, 1, 100, 20);
    let exact = caster_at(2, 1, 10, 4);
    let entity = game.world.resource::<EntityIndex>().get(spent).unwrap();
    game.world
        .get_mut::<Pools>(entity)
        .unwrap()
        .take(MANA, num(91));
    let enemy = game.spawn(1, at(num(5), Num::ZERO, Num::ZERO), ());
    let far = game.spawn(1, at(num(6), Num::ZERO, Num::ZERO), ());
    let ally = game.spawn(0, at(num(1), Num::ZERO, num(1)), ());
    let hidden = game.spawn(1, at(num(1), Num::ZERO, Num::ZERO), ());
    game.set_blocks(hidden, &[Block::Target]);

    // An ally, an untargetable enemy, a unit beyond 5 m, and no target at all are refused; so
    // are a slot not learned, and a cost of 10 mana and 4 rage against 5 mana, 3 rage, or 9
    // mana left of 100. A refused cast spends no mana or rage, and starts no cooldown.
    let spending = |game: &Match, unit| {
        (
            game.pool(unit),
            game.pool_of(unit, RAGE),
            game.slot(unit).ready_at,
        )
    };
    for (unit, target) in [
        (caster, ActionTarget::Unit(ally)),
        (caster, ActionTarget::Unit(hidden)),
        (caster, ActionTarget::Unit(far)),
        (caster, ActionTarget::None),
        (unlearned, ActionTarget::Unit(enemy)),
        (poor, ActionTarget::Unit(enemy)),
        (calm, ActionTarget::Unit(enemy)),
        (spent, ActionTarget::Unit(enemy)),
    ] {
        let before = spending(&game, unit);
        game.cast(unit, target);
        assert_eq!(game.health(enemy), 500, "{unit:?} at {target:?}");
        assert_eq!(spending(&game, unit), before, "{unit:?} at {target:?}");
    }
    let healths = [far, ally, hidden].map(|unit| game.health(unit));
    assert_eq!(healths, [500, 500, 500]);
    assert_eq!(game.pool(caster), 100);

    // At exactly 5 m, the enemy takes 50, and the cost comes off each pool: 100 − 10 mana and
    // 20 − 4 rage. The cooldown, 1001 ms, is 30.03 ticks, rounded up to 31: the cast in tick 8 is
    // ready again in tick 39.
    game.cast(caster, ActionTarget::Unit(enemy));
    assert_eq!(game.health(enemy), 450);
    assert_eq!((game.pool(caster), game.pool_of(caster, RAGE)), (90, 16));
    assert_eq!(game.slot(caster).ready_at, Tick::new(39));
    // A cost of exactly what is left is paid: 10 mana and 4 rage of 10 and 4, in tick 9.
    game.cast(exact, ActionTarget::Unit(enemy));
    assert_eq!(game.health(enemy), 400);
    assert_eq!(spending(&game, exact), (0, 0, Tick::new(40)));

    // The range counts from the edge of each body: once the unit 6 m off has a body of 1 m, it
    // is within 5 m, and takes 50 in tick 39.
    let far_entity = game.world.resource::<EntityIndex>().get(far).unwrap();
    game.world
        .entity_mut(far_entity)
        .insert(Body::new(Num::ONE).unwrap());
    game.run_until(39);
    game.cast(caster, ActionTarget::Unit(far));
    assert_eq!(game.health(far), 450);
}

#[test]
fn a_cost_in_a_pool_and_a_player_resource_is_checked_and_paid_together() {
    let mut game = Match::new();
    let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
    let mut cost = cost("mana", int(10));
    cost.insert(DeclaredName::new("gold").unwrap(), Ranked::One(int(30)));
    let data = ActionData { cost, ..strike() };
    let strike = game.load("strike", &data, STRIKE);
    let caster = game.caster(strike, 1);
    let ownerless = game.spawn(
        0,
        at(Num::ZERO, Num::ZERO, num(1)),
        ActionSlots::new([(strike, SlotKind::new(0), 1)]),
    );
    game.give_pools(ownerless, 100, 20);
    let enemy = game.spawn(1, at(num(5), Num::ZERO, Num::ZERO), ());
    let player = PlayerSlot::new(0);
    game.world
        .resource_mut::<PlayerResources>()
        .add(player, gold, 40)
        .unwrap();
    let held = |game: &Match, unit| {
        let amounts = game.world.resource::<PlayerResources>();
        (game.pool(unit), amounts.amount(player, gold))
    };
    // A unit no player owns pays no player resource, so it may not cast.
    game.cast(ownerless, ActionTarget::Unit(enemy));
    assert_eq!(game.health(enemy), 500);
    assert_eq!(game.pool(ownerless), 100);
    // Player 0's caster pays both in tick 1, 100 − 10 mana and 40 − 30 gold, as the strike
    // lands.
    game.cast(caster, ActionTarget::Unit(enemy));
    assert_eq!(game.health(enemy), 450);
    assert_eq!(held(&game, caster), (90, 10));
    // Ready again in tick 1 + 31 = 32, the cooldown's 1001 ms in ticks rounded up, it may not
    // cast with 10 gold of 30: nothing is spent, and the cooldown does not start again.
    game.run_until(32);
    game.cast(caster, ActionTarget::Unit(enemy));
    assert_eq!(game.health(enemy), 450);
    assert_eq!(held(&game, caster), (90, 10));
    assert_eq!(game.slot(caster).ready_at, Tick::new(32));
}

#[test]
fn a_cast_its_casters_tags_stop_is_kept_and_an_interrupted_one_spends_nothing() {
    let mut game = Match::new();
    // Strike with a windup of 100 ms, 3 ticks.
    let data = ActionData {
        kind: ActionKind::Cast,
        windup_ms: Some(Ranked::One(int(100))),
        ..strike()
    };
    let strike = game.load("strike", &data, STRIKE);
    let caster = game.caster(strike, 1);
    let enemy = game.spawn(1, at(num(5), Num::ZERO, Num::ZERO), ());
    let target = ActionTarget::Unit(enemy);
    let order = InProgress {
        slot: 0,
        kind: ActionKind::Cast,
        target,
        resolves_at: None,
    };
    let ordered = Some(order);
    let started = |tick| {
        Some(InProgress {
            resolves_at: Some(Tick::new(tick)),
            ..order
        })
    };
    // A stun in tick 7's Move stage, after the casts start in Act and before they resolve in Hit.
    let caster_entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    let stun = move |tick: Res<'_, SimTick>, mut tags: Query<'_, '_, &mut UnitTags>| {
        if tick.start() == Tick::new(7) {
            *tags.get_mut(caster_entity).unwrap() =
                UnitTags::blocking(&[Block::Move, Block::Attack, Block::Cast, Block::Use]);
        }
    };
    game.world.schedule_scope(SimUpdate, |_, schedule| {
        schedule.add_systems(stun.in_set(SimSet::Move));
    });

    // Silenced in tick 0 and 1: the order is kept, and not started.
    game.set_blocks(caster, &[Block::Cast]);
    game.cast(caster, target);
    game.run_until(2);
    assert_eq!(game.casting(caster), ordered);
    // Free in tick 2: it starts, to resolve in tick 5. Silenced again in tick 3: its windup
    // is interrupted, back to the order, and spends nothing.
    game.set_blocks(caster, &[]);
    game.run_until(3);
    assert_eq!(game.casting(caster), started(5));
    game.set_blocks(caster, &[Block::Cast]);
    game.run_until(4);
    assert_eq!(game.casting(caster), ordered);
    // Free in tick 4: it starts again, to resolve in tick 7, when the stun in Move holds it
    // back from resolving: back to the order once more.
    game.set_blocks(caster, &[]);
    game.run_until(8);
    assert_eq!(game.casting(caster), ordered);
    assert_eq!((game.health(enemy), game.pool(caster)), (500, 100));
    assert_eq!(game.slot(caster).ready_at, Tick::new(0));
    // Free in tick 8: it starts and resolves in tick 11, for 50 and 10 of the pool; ready again
    // 31 ticks later, in tick 42.
    game.set_blocks(caster, &[]);
    game.run_until(12);
    assert_eq!(game.casting(caster), None);
    assert_eq!((game.health(enemy), game.pool(caster)), (450, 90));
    assert_eq!(game.slot(caster).ready_at, Tick::new(42));
}

#[test]
fn a_failed_script_changes_nothing_and_fails_the_same_way_everywhere() {
    let spin = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "magic"); } loop {} }"#;
    let wrong_kind = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "fire"); } }"#;
    let overflow = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "magic"); } num(1 << 20) * num(1 << 20) }"#;
    let undeclared = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "magic"); } ctx.p.radius }"#;
    let data = ActionData {
        kind: ActionKind::Cast,
        params: BTreeMap::new(),
        ..lash_out()
    };
    let cases: [(&str, fn(&CallError) -> bool); 4] = [
        (spin, |error| {
            matches!(error, CallError::Script(ScriptError::CallLimit))
        }),
        (wrong_kind, |error| {
            matches!(error, CallError::Api(ApiError::UnknownDamageKind))
        }),
        (undeclared, |error| {
            matches!(error, CallError::Api(ApiError::UnknownParam))
        }),
        (overflow, |error| match error {
            CallError::Script(ScriptError::Raised(raised)) => {
                raised.get::<NumError>() == Some(NumError::Overflow)
            }
            _ => false,
        }),
    ];
    for (script, expected) in cases {
        // Two matches alike, but only one gets the cast order.
        let mut hashes = Vec::new();
        let mut spent = Vec::new();
        for ordered in [true, false] {
            let mut game = Match::new();
            let ability = game.load("ability", &data, script);
            let caster = game.caster(ability, 1);
            let enemy = game.spawn(1, at(num(1), Num::ZERO, Num::ZERO), ());
            if ordered {
                game.cast(caster, ActionTarget::None);
                let failures = game.failures();
                assert_eq!(failures.len(), 1, "{script}");
                assert_eq!(failures[0].unit, Some(caster));
                assert_eq!(failures[0].hook, Hook::OnResolve);
                assert!(expected(&failures[0].error), "{:?}", failures[0].error);
                let mut budgets = game.world.resource_mut::<ScriptBudgets>();
                spent
                    .push(LIMITS.player - budgets.get_mut(Pool::Player(PlayerSlot::new(0))).left());
            } else {
                game.world.run_schedule(SimUpdate);
            }
            assert_eq!(game.health(enemy), 500);
            assert_eq!(game.pool(caster), 100);
            assert_eq!(game.slot(caster).ready_at, Tick::new(0));
            hashes.push(game.registry.hash(&game.world));
        }
        assert_eq!(hashes[0], hashes[1], "{script}");
        if script == spin {
            // The call ran exactly its limit of operations.
            assert_eq!(spent, [LIMITS.per_call]);
        }
    }
}

#[test]
fn a_cast_draws_from_its_casters_player_pool() {
    // Each player's pool holds one whole call: player 0's spinning cast spends all of its pool,
    // and player 1's strike in the same tick still resolves from its own, which it spends as it
    // does alone.
    let mut left = Vec::new();
    for spins in [true, false] {
        let limits = ScriptLimits {
            player: LIMITS.per_call,
            ..LIMITS
        };
        let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
        let mut game = Match::with(limits, &declared);
        let data = ActionData {
            kind: ActionKind::Cast,
            params: BTreeMap::new(),
            ..lash_out()
        };
        let spin = game.load(
            "spin",
            &data,
            "fn on_resolve(ctx, caster, target) { loop {} }",
        );
        let strike = game.load("strike", &strike(), STRIKE);
        let spinner = game.caster(spin, 1);
        let striker = game.spawn(
            0,
            at(Num::ZERO, Num::ZERO, num(1)),
            (
                Owner::new(PlayerSlot::new(1)),
                ActionSlots::new([(strike, SlotKind::new(0), 1)]),
            ),
        );
        game.give_pools(striker, 100, 20);
        let enemy = game.spawn(1, at(num(1), Num::ZERO, Num::ZERO), ());
        let casts = [
            (spinner, ActionTarget::None),
            (striker, ActionTarget::Unit(enemy)),
        ];
        game.casts(if spins { &casts } else { &casts[1..] });
        let failures = game.failures();
        assert_eq!(failures.len(), usize::from(spins));
        if spins {
            assert_eq!(failures[0].unit, Some(spinner));
            assert!(matches!(
                failures[0].error,
                CallError::Script(ScriptError::CallLimit)
            ));
        }
        // The strike's 50 true damage: 500 → 450, and its cost of 10: 100 → 90.
        assert_eq!(game.health(enemy), 450);
        assert_eq!(game.pool(striker), 90);
        let mut budgets = game.world.resource_mut::<ScriptBudgets>();
        let spent = if spins { 0 } else { LIMITS.per_call };
        assert_eq!(
            budgets.get_mut(Pool::Player(PlayerSlot::new(0))).left(),
            spent
        );
        left.push(budgets.get_mut(Pool::Player(PlayerSlot::new(1))).left());
    }
    assert_eq!(left[0], left[1]);
    assert!(left[0] < LIMITS.per_call);
}

#[test]
fn an_ability_loads_only_when_its_data_holds() {
    let mut game = Match::new();
    game.load_stats();
    let load = |game: &mut Match, data: &ActionData, source: &str| {
        let script = Units::compile(&mut game.world, source).unwrap();
        Actions::load(&mut game.world, 0, "lash_out", data, Some(script), 5)
    };
    let mut uneven = lash_out();
    uneven.cost = BTreeMap::from([(
        DeclaredName::new("mana").unwrap(),
        Ranked::PerRank(vec![int(35), int(40)]),
    )]);
    let mut aimed = lash_out();
    aimed.targeting = Targeting::Direction;
    let mut forever = lash_out();
    forever.cooldown_ms = Some(Ranked::One(int(i64::MAX)));
    let mut scaled = lash_out();
    scaled.cooldown_ms = Some(Ranked::One(param("damage")));
    let mut negative = lash_out();
    negative.cost = cost("mana", int(-1));
    let mut poolless = lash_out();
    poolless.cost = cost("focus", int(1));
    let mut unknown = lash_out();
    unknown.range = Some(Ranked::One(RangeField::Param(ParamRef {
        param: DeclaredName::new("reach").unwrap(),
    })));
    // The data's rules, which the package load checks: two costs for five ranks, and the field
    // that does not hold at rank 1. A scaling param is the caster's value, which no cooldown may
    // take.
    assert!(lash_out().check_ranks(5) && !uneven.check_ranks(5));
    for (data, field) in [
        (scaled, ActionField::Cooldown),
        (negative, ActionField::Cost),
        (poolless, ActionField::Cost),
        (unknown, ActionField::Range),
    ] {
        let pool = |name: &DeclaredName| {
            let at = POOLS.iter().position(|pool| *pool == name.as_str())?;
            PoolId::new(u8::try_from(at).unwrap()).map(CostTarget::Pool)
        };
        assert_eq!(data.fields_at(1, pool).err(), Some(field), "{field:?}");
    }
    // What only a match's rate decides: i64::MAX ms counts in no tick.
    assert!(matches!(
        load(&mut game, &forever, LASH_OUT),
        Err(ActionError::TimeTooLarge)
    ));
    assert!(load(&mut game, &lash_out(), LASH_OUT).is_ok());
    // A direction loads, as every targeting does; no cast can aim one yet.
    assert!(load(&mut game, &aimed, LASH_OUT).is_ok());

    // A script may serve only the ability's modifiers: a cast of rank 1 in tick 0 then runs no
    // script, and spends 35 of 100 and its 10 000 ms, 300 ticks at 30 a second.
    let modifiers_only = "fn on_damage_taken(ctx, m, d) { }";
    let passive = load(&mut game, &lash_out(), modifiers_only).unwrap();
    let caster = game.caster(passive, 1);
    game.cast(caster, ActionTarget::None);
    assert!(game.failures().is_empty());
    assert_eq!(game.pool(caster), 65);
    assert_eq!(game.slot(caster).ready_at, Tick::new(300));
}

#[test]
fn a_capability_field_reads_its_param_at_each_rank() {
    // A cooldown of `{ param = "cd" }`, 1000 ms at rank 1 up to 3000 at rank 3: 30, 60 and 90
    // ticks at 30 a second, and a cost of `{ param = "price" }` mana, 7 at every rank.
    let mut data = strike();
    data.cooldown_ms = Some(Ranked::One(param("cd")));
    data.cost = BTreeMap::from([(
        DeclaredName::new("mana").unwrap(),
        Ranked::PerRank(vec![int(5), param("price"), int(9)]),
    )]);
    let per_rank = |values: &[i64]| {
        Param::Ranked(Ranked::PerRank(
            values.iter().map(|&value| Scalar::Int(value)).collect(),
        ))
    };
    data.params.insert(
        DeclaredName::new("cd").unwrap(),
        per_rank(&[1000, 2000, 3000]),
    );
    data.params.insert(
        DeclaredName::new("price").unwrap(),
        Param::Ranked(Ranked::One(Scalar::Int(7))),
    );
    let mut game = Match::new();
    let strike = Units::compile(&mut game.world, STRIKE).unwrap();
    let id = Actions::load(&mut game.world, 0, "strike", &data, Some(strike), 3).unwrap();
    let book = game.world.resource::<ActionBook>();
    let ranks: Vec<_> = book
        .get(id)
        .unwrap()
        .ranks
        .iter()
        .map(|values| (values.cooldown.get(), values.cost.get(MANA).round()))
        .collect();
    assert_eq!(ranks, [(30, 5), (60, 7), (90, 9)]);
    // Three ranks of an array of 3 is the rank count; five is not.
    assert!(data.check_ranks(3) && !data.check_ranks(5));
}

#[test]
fn a_unit_target_is_one_its_filter_selects_tag_and_all() {
    let mut game = Match::new();
    let mut load_type = |name: &str| {
        let data = UnitTypeData {
            tags: vec![DeclaredName::new(name).unwrap()],
            params: BTreeMap::new(),
        };
        Units::load_type(&mut game.world, TypeScope::Mode, name, &data)
    };
    let (hero, creep) = (load_type("avatar"), load_type("creep"));
    let mut heroes_only = strike();
    heroes_only.targeting = Targeting::Unit(FilterData::parse("enemies:avatar").unwrap());
    let strike = game.load("strike", &heroes_only, STRIKE);
    let caster = game.caster(strike, 1);
    let enemy_creep = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), creep);
    let enemy_hero = game.spawn(1, at(num(4), Num::ZERO, Num::ZERO), hero);
    // The creep is an enemy, but no hero: the cast goes nowhere. The hero takes 50.
    game.cast(caster, ActionTarget::Unit(enemy_creep));
    assert_eq!(game.health(enemy_creep), 500);
    game.cast(caster, ActionTarget::Unit(enemy_hero));
    assert_eq!(game.health(enemy_hero), 450);
}

#[test]
fn a_passive_is_held_while_its_ability_has_a_rank_and_is_ready() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Match::with(LIMITS, &declared);
    game.load_stats();
    // A guard whose shield is Lash Out's damage at its rank: 75, then 100.
    let guard = ModifierData {
        script: None,
        duration_ms: None,
        interval_ms: None,
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: BTreeMap::new(),
        tags: Vec::new(),
        shield: Some(param("damage")),
        aura: None,
        affects: None,
        params: BTreeMap::new(),
        state: BTreeMap::new(),
    };
    Stats::load_modifier(&mut game.world, 0, "guard", &guard, None);
    let mut data = lash_out();
    data.passive_modifier = Some(DeclaredName::new("guard").unwrap());
    data.passive_while_ready = true;
    let ability = game.load("lash_out", &data, LASH_OUT);
    let caster = game.caster(ability, 0);
    let entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    game.world.entity_mut(entity).insert(Modifiers::default());
    let id = game
        .world
        .resource::<ModifierBook>()
        .named(0, "guard")
        .unwrap();
    let shield = |game: &Match| {
        let modifiers = game.world.get::<Modifiers>(entity).unwrap();
        let held = modifiers.get(id, Some(caster))?;
        assert!(held.passive && held.until.is_none());
        held.shield
    };
    let learn = |game: &mut Match| {
        let mut slots = game.world.get_mut::<ActionSlots>(entity).unwrap();
        slots.learn(0);
        game.world.run_schedule(SimUpdate);
    };
    // Unlearned, none; at rank 1, the shield of 75; at rank 2, applied again, 100.
    game.world.run_schedule(SimUpdate);
    assert_eq!(shield(&game), None);
    learn(&mut game);
    assert_eq!(shield(&game), Some(num(75)));
    learn(&mut game);
    assert_eq!(shield(&game), Some(num(100)));
    // A cast puts it on cooldown for 9000 ms at rank 2, 270 ticks: gone from that tick, back in
    // the 270th after it.
    game.cast(caster, ActionTarget::None);
    assert_eq!(shield(&game), None);
    for _ in 0..269 {
        game.world.run_schedule(SimUpdate);
    }
    assert_eq!(shield(&game), None);
    game.world.run_schedule(SimUpdate);
    assert_eq!(shield(&game), Some(num(100)));

    // A weapon's passive holds as well, in a match with no abilities: a ward of 40 from its
    // first tick.
    let mut game = Match::with(LIMITS, &[Capability::Stats, Capability::Combat]);
    game.load_stats();
    let ward = ModifierData {
        shield: Some(int(40)),
        ..guard
    };
    Stats::load_modifier(&mut game.world, 0, "ward", &ward, None);
    let mut claws = lash_out();
    claws.kind = ActionKind::Attack;
    claws.script = None;
    claws.cooldown_ms = None;
    claws.cost = BTreeMap::new();
    claws.params = BTreeMap::new();
    claws.targeting = Targeting::Unit(FilterData::parse("enemies").unwrap());
    claws.range = Some(Ranked::One(RangeField::Range(Range::Meters(num(2)))));
    claws.rate = Some(Stat::named("armor").unwrap());
    claws.damage = Some(Stat::named("attack_damage").unwrap());
    claws.damage_kind = Some(DeclaredName::new("physical").unwrap());
    claws.passive_modifier = Some(DeclaredName::new("ward").unwrap());
    let claws = Actions::load(&mut game.world, 0, "claws", &claws, None, 1).unwrap();
    let slots = ActionSlots::new([(claws, SlotKind::new(0), 1)]);
    let beast = game.spawn(0, at(Num::ZERO, Num::ZERO, Num::ZERO), slots);
    let entity = game.world.resource::<EntityIndex>().get(beast).unwrap();
    game.world.entity_mut(entity).insert(Modifiers::default());
    game.world.run_schedule(SimUpdate);
    let ward = game
        .world
        .resource::<ModifierBook>()
        .named(0, "ward")
        .unwrap();
    let modifiers = game.world.get::<Modifiers>(entity).unwrap();
    assert_eq!(
        modifiers.get(ward, Some(beast)).map(|held| held.shield),
        Some(Some(num(40)))
    );
}

#[test]
fn a_cast_applies_a_modifier_from_its_caster_with_its_abilitys_params() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Match::with(LIMITS, &declared);
    game.load_stats();
    let mark = ModifierData {
        script: None,
        duration_ms: Some(int(1000)),
        interval_ms: None,
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: BTreeMap::new(),
        tags: Vec::new(),
        shield: Some(param("damage")),
        aura: None,
        affects: None,
        params: BTreeMap::new(),
        state: BTreeMap::new(),
    };
    Stats::load_modifier(&mut game.world, 0, "mark", &mark, None);
    // Package 1 names its own `mark`, of no shield.
    let other = ModifierData {
        shield: None,
        ..mark.clone()
    };
    Stats::load_modifier(&mut game.world, 1, "mark", &other, None);
    let marker = r#"
fn on_resolve(ctx, caster, target) {
    let m = ctx.add_modifier(caster, "mark");
    if m.stacks != 1 { throw "a new modifier's handle has one stack"; }
}
"#;
    let ability = game.load("lash_out", &lash_out(), marker);
    let caster = game.caster(ability, 3);
    let entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    game.world.entity_mut(entity).insert(Modifiers::default());
    let t = game.world.resource::<SimTick>().start();
    game.cast(caster, ActionTarget::None);
    // From the caster, by Lash Out at rank 3: a shield of its damage there, 125; 1000 ms at 30
    // ticks a second, 30 ticks, so it ends as tick t + 31 starts.
    let id = game
        .world
        .resource::<ModifierBook>()
        .named(0, "mark")
        .unwrap();
    let modifiers = game.world.get::<Modifiers>(entity).unwrap();
    let held = modifiers.get(id, Some(caster)).unwrap();
    assert_eq!((held.ability, held.rank), (Some(ability), 3));
    assert_eq!(
        (held.shield, held.until),
        (Some(num(125)), Some(Tick::new(t.get() + 31)))
    );

    // A cast of package 1's ability means package 1's names: the caster carries package 0's
    // `mark`, not its own, and the cast adds its own beside it.
    let own = r#"
fn on_resolve(ctx, caster, target) {
    if caster.has_modifier("mark") { throw "the caster carries no mark of package 1"; }
    ctx.add_modifier(caster, "mark");
}
"#;
    let script = Units::compile(&mut game.world, own).unwrap();
    let theirs = Actions::load(&mut game.world, 1, "lash_out", &lash_out(), Some(script), 5);
    let slots = ActionSlots::new([(theirs.unwrap(), SlotKind::new(0), 1)]);
    game.world.entity_mut(entity).insert(slots);
    game.cast(caster, ActionTarget::None);
    assert!(game.failures().is_empty(), "{:?}", game.failures());
    let book = game.world.resource::<ModifierBook>();
    let ids = [0, 1].map(|package| book.named(package, "mark").unwrap());
    let modifiers = game.world.get::<Modifiers>(entity).unwrap();
    let shields = ids.map(|id| modifiers.get(id, Some(caster)).map(|held| held.shield));
    assert_eq!(shields, [Some(Some(num(125))), Some(None)]);
}

/// A match in which a strike stuns its target for 100 ms through the mode's `stunned` tag: each
/// tick's state hash, and whether the target's tags block its moving.
fn stun_run() -> Vec<(StateHash, bool)> {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Match::with(LIMITS, &declared);
    game.load_stats();
    let stun = ModifierData {
        script: None,
        tags: vec![DeclaredName::new("stunned").unwrap()],
        ..scripted(None, &[])
    };
    Stats::load_modifier(&mut game.world, 0, "stun", &stun, None);
    let stunned = TagData {
        blocks: vec![Block::Move, Block::Attack, Block::Cast, Block::Use],
        ..TagData::default()
    };
    let effects = BTreeMap::from([(DeclaredName::new("stunned").unwrap(), stunned)]);
    let book = game.world.non_send::<View>().types_mut().tag_book(&effects);
    Units::load_tags(&mut game.world, book);
    let script = r#"fn on_resolve(ctx, caster, target) { ctx.add_modifier(target, "stun", 100); }"#;
    let strike = game.load("strike", &strike(), script);
    let caster = game.caster(strike, 1);
    let target_type = Units::load_type(
        &mut game.world,
        TypeScope::Mode,
        "target",
        &UnitTypeData::default(),
    );
    let parts = (
        target_type,
        Level::default(),
        UnitStats::default(),
        UnitTags::default(),
        Modifiers::default(),
    );
    let enemy = game.spawn(1, at(num(5), Num::ZERO, Num::ZERO), parts);
    let entity = game.world.resource::<EntityIndex>().get(enemy).unwrap();
    let mut seen = Vec::new();
    for tick in 0..8 {
        if tick == 2 {
            game.cast(caster, ActionTarget::Unit(enemy));
        } else {
            game.world.run_schedule(SimUpdate);
        }
        let stunned = UnitTags::effects_of(game.world.get(entity)).blocks(Block::Move);
        seen.push((game.registry.hash(&game.world), stunned));
    }
    seen
}

#[test]
fn a_stun_a_script_applies_blocks_its_target_alike_in_every_run() {
    // The strike resolves in tick 2's Hit and stuns for 100 ms, 3 ticks: the stun holds through
    // tick 2 + 3 = 5, and ends as tick 6 starts.
    let first = stun_run();
    let stunned: Vec<_> = first.iter().map(|&(_, stunned)| stunned).collect();
    assert_eq!(
        stunned,
        [false, false, true, true, true, true, false, false]
    );
    assert_eq!(stun_run(), first);
}

#[test]
fn a_cast_heals_and_restores_and_a_negative_amount_fails_it() {
    let mut game = Match::new();
    game.load_stats();
    let mender = "
fn on_resolve(ctx, caster, target) {
    ctx.heal(caster, 30);
    ctx.restore(caster, \"mana\", num(20));
}
";
    let ability = game.load("lash_out", &lash_out(), mender);
    let caster = game.caster(ability, 1);
    let entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    let mut pools = game.world.get_mut::<Pools>(entity).unwrap();
    pools.take(PoolId::FIRST, num(460));
    pools.take(MANA, num(50));
    // From 40 health and 50 mana: 30 healed and 20 restored as the effects apply, then the
    // cost of 35: 70 and 35.
    game.cast(caster, ActionTarget::None);
    assert_eq!((game.health(caster), game.pool(caster)), (70, 35));

    let mut game = Match::new();
    game.load_stats();
    let negative = "fn on_resolve(ctx, caster, target) { ctx.restore(caster, \"mana\", 5); ctx.heal(caster, -1); }";
    let ability = game.load("lash_out", &lash_out(), negative);
    let caster = game.caster(ability, 1);
    game.cast(caster, ActionTarget::None);
    let refused = game.failures().iter().map(|failure| &failure.error);
    assert!(
        refused
            .clone()
            .all(|error| matches!(error, CallError::Api(ApiError::NegativeHeal)))
            && refused.count() == 1
    );
    assert_eq!(game.pool(caster), 100);
}

/// A modifier of no duration that runs `script`, with an interval of `interval_ms` and the
/// params `params`.
fn scripted(interval_ms: Option<i64>, params: &[(&str, i64)]) -> ModifierData {
    ModifierData {
        script: Some(PackagePath::parse("scripts/hooks.rhai").unwrap()),
        duration_ms: None,
        interval_ms: interval_ms.map(int),
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: BTreeMap::new(),
        tags: Vec::new(),
        shield: None,
        aura: None,
        affects: None,
        params: params
            .iter()
            .map(|&(name, value)| {
                (
                    DeclaredName::new(name).unwrap(),
                    Param::Ranked(Ranked::One(Scalar::Int(value))),
                )
            })
            .collect(),
        state: BTreeMap::new(),
    }
}

impl Match {
    /// A match of stats, combat and abilities whose modifiers, by name in package 0, run
    /// `source`, as `scripted` declares them.
    fn with_modifiers(source: &str, modifiers: &[(&str, ModifierData)]) -> Match {
        let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
        let mut game = Match::with(LIMITS, &declared);
        game.load_stats();
        let script = Units::compile(&mut game.world, source).unwrap();
        let mut sorted = modifiers.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (name, data) in &sorted {
            Stats::load_modifier(&mut game.world, 0, name, data, Some(script));
        }
        game
    }

    /// Gives `unit` the modifier `name` from itself.
    fn give(&mut self, unit: StableId, name: &str) {
        let id = Stats::modifier(&self.world, 0, name).unwrap();
        let entity = self.world.resource::<EntityIndex>().get(unit).unwrap();
        if !self.world.entity(entity).contains::<Modifiers>() {
            self.world.entity_mut(entity).insert(Modifiers::default());
        }
        let applier = Applier {
            source: Some(unit),
            ability: None,
            rank: 1,
            passive: false,
            held: false,
        };
        let add = ModifierEffect::Add {
            target: unit,
            id,
            duration: None,
        };
        Stats::apply_effect(&mut self.world, add, applier);
    }

    /// Each failed call of the tick: the unit it ran for and its hook.
    fn calls(&self) -> Vec<(StableId, Hook)> {
        let failures = self.failures().iter();
        failures
            .map(|failure| (failure.unit.unwrap(), failure.hook))
            .collect()
    }
}

#[test]
fn combat_events_reach_each_modifier_once_in_their_order() {
    // Every hook fails, so the failures list the calls in the order they ran.
    let log = r#"
fn on_attack(ctx, m, target) { throw "attack"; }
fn on_attack_hit(ctx, m, d) { throw "hit"; }
fn on_damage_taken(ctx, m, d) { throw "taken"; }
fn on_kill(ctx, m, victim) { throw "kill"; }
fn on_takedown(ctx, m, victim) { throw "takedown"; }
fn on_interval(ctx, m) { throw "interval"; }
"#;
    let mut game = Match::with_modifiers(
        log,
        &[
            ("log", scripted(None, &[])),
            ("pulse", scripted(Some(100), &[])),
        ],
    );
    game.world.insert_resource(AssistWindow(Ticks::new(10)));
    let victim = game.spawn(1, at(Num::ONE, Num::ZERO, Num::ZERO), ());
    let origin = at(Num::ZERO, Num::ZERO, Num::ZERO);
    let attacker = game.attacker(0, origin, striker(500), victim);
    let assister = game.spawn(0, at(num(3), Num::ZERO, Num::ZERO), ());
    let bystander = game.spawn(1, at(num(9), Num::ZERO, Num::ZERO), ());
    for unit in [attacker, assister, victim] {
        game.give(unit, "log");
    }
    game.give(bystander, "pulse");
    // The attacker strikes for 500 as its windup of no ticks ends, in tick 0; the assister
    // struck the victim in tick 0 too.
    let target = game.world.resource::<EntityIndex>().get(victim).unwrap();
    game.world
        .resource_scope(|world, index: Mut<'_, EntityIndex>| {
            let mut attackers = world.get_mut::<RecentAttackers>(target).unwrap();
            attackers.record(assister, Tick::new(0), &index);
        });
    game.world.run_schedule(SimUpdate);
    assert_eq!(game.health(victim), 0);
    assert_eq!(
        game.calls(),
        [
            (attacker, Hook::OnAttack),
            (attacker, Hook::OnAttackHit),
            (victim, Hook::OnDamageTaken),
            (attacker, Hook::OnKill),
            (attacker, Hook::OnTakedown),
            (assister, Hook::OnTakedown),
        ]
    );
    // The pulse of 100 ms, 3 ticks at 30 a second, applied in tick 0: in ticks 3 and 6 alone.
    for tick in 1..=6 {
        game.world.run_schedule(SimUpdate);
        let expected = if tick % 3 == 0 {
            vec![(bystander, Hook::OnInterval)]
        } else {
            Vec::new()
        };
        assert_eq!(game.calls(), expected, "tick {tick}");
    }
}

#[test]
fn a_hook_deals_damage_into_the_pass_and_a_chain_16_deep_fails() {
    // An extra attack on each attack that is not one, and a unit that hurts itself again, by its
    // modifier's `echo`, each time it takes damage. An attack and an extra attack name the
    // weapon, the tests' `weapon`; the damage of no action, and an echo's, name none.
    let hooks = r#"
fn on_attack_hit(ctx, m, d) {
    if d.ability != "weapon" {
        throw "an attack names its weapon";
    }
    if !d.extra {
        ctx.attack_hit(d.target);
    }
}
fn on_damage_taken(ctx, m, d) {
    if d.ability != () {
        throw "a damage of no action names none";
    }
    ctx.damage(m.carrier, ctx.p.echo, "true");
}
"#;
    let mut game = Match::with_modifiers(
        hooks,
        &[
            ("double", scripted(None, &[])),
            ("echo", scripted(None, &[("echo", 1)])),
        ],
    );
    let victim = game.spawn(1, at(Num::ONE, Num::ZERO, Num::ZERO), ());
    let origin = at(Num::ZERO, Num::ZERO, Num::ZERO);
    let attacker = game.attacker(0, origin, striker(30), victim);
    let echoer = game.spawn(1, at(num(9), Num::ZERO, Num::ZERO), ());
    game.give(attacker, "double");
    game.give(echoer, "echo");
    game.world.resource_mut::<PassQueue>().push_damage(Damage {
        source: None,
        target: echoer,
        amount: num(10),
        kind: DamageKind::new(2),
        cause: DamageCause::Effect,
        ability: None,
        depth: 0,
        hit: None,
    });
    game.world.run_schedule(SimUpdate);
    // The attack's 30, then the extra attack's 30, which adds none: 500 − 60. The echoer's 10,
    // then an echo of 1 from each hook at depths 1 to 15; the one at 16 fails: 500 − 10 − 15.
    assert_eq!(game.health(victim), 440);
    assert_eq!(game.health(echoer), 475);
    let failures: Vec<_> = game
        .failures()
        .iter()
        .map(|failure| (failure.unit, failure.hook, failure.error.clone()))
        .collect();
    assert!(matches!(
        failures.as_slice(),
        [(Some(unit), Hook::OnDamageTaken, CallError::Api(ApiError::ChainTooDeep))] if *unit == echoer
    ));
}

/// The stats of the scaling tests' mode, in its order: attack damage, an engine stat, first.
fn scaling_stats() -> [Stat; 4] {
    let mut stats = ["attack_damage", "ability_power", "armor", "spell_vamp"]
        .map(|name| Stat::named(name).unwrap());
    stats.sort();
    stats
}

/// A modifier of no script that changes `stats`, each by an add, and reads `params`, lasting
/// `duration_ms` when given.
fn changing(
    stats: &[(&str, Number)],
    params: &[(&str, Param)],
    duration_ms: Option<Number>,
) -> ModifierData {
    let change = |value: &Number| StatChange {
        op: StatOp::Add,
        value: value.clone(),
    };
    ModifierData {
        script: None,
        duration_ms,
        interval_ms: None,
        stacks_expire_ms: None,
        reapply: Reapply::Refresh,
        max_stacks: None,
        stats: stats
            .iter()
            .map(|(name, value)| (Stat::named(name).unwrap(), change(value)))
            .collect(),
        tags: Vec::new(),
        shield: None,
        aura: None,
        affects: None,
        params: params
            .iter()
            .map(|(name, param)| (DeclaredName::new(name).unwrap(), param.clone()))
            .collect(),
        state: BTreeMap::new(),
    }
}

/// A scaling table of `base` at every rank, `per_level`, and `ratios` and `bonus` by stat name.
fn scaling(base: Scalar, per_level: i64, ratios: &[(&str, Num)], bonus: &[(&str, Num)]) -> Param {
    let by_name = |pairs: &[(&str, Num)]| {
        pairs
            .iter()
            .map(|&(name, ratio)| (Stat::named(name).unwrap(), ratio))
            .collect()
    };
    Param::Scaling(Scaling {
        base: Ranked::One(base),
        per_level: Num::from_int(per_level).unwrap(),
        ratios: by_name(ratios),
        bonus: by_name(bonus),
    })
}

fn decimal(text: &str) -> Num {
    text.parse().unwrap()
}

#[test]
fn a_scaling_param_reads_its_sources_level_stats_and_bonus() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Match::with(LIMITS, &declared);
    // The caster's type gives attack damage 50 + 5 a level, so 60 at level 3; a modifier adds 20
    // more and 40 ability power.
    let caster_type = Units::load_type(
        &mut game.world,
        TypeScope::Mode,
        "caster",
        &UnitTypeData::default(),
    );
    let attack_damage = Stat::named("attack_damage").unwrap();
    let growth = StatValue {
        base: Num::from_int(50).unwrap(),
        per_level: Num::from_int(5).unwrap(),
    };
    let growth = StatsData([(attack_damage, growth)].into());
    let rules: BTreeMap<_, _> = scaling_stats()
        .map(|stat| (stat, StatRule::default()))
        .into();
    let book = StatBook::new(&rules, [(caster_type, &growth)], num(6));
    Stats::load_book(&mut game.world, book);
    let boost = changing(
        &[("attack_damage", int(20)), ("ability_power", int(40))],
        &[],
        None,
    );
    Stats::load_modifier(&mut game.world, 0, "boost", &boost, None);
    let mark = changing(&[], &[], Some(param("power")));
    Stats::load_modifier(&mut game.world, 0, "mark", &mark, None);
    // Power: 100 a rank, 10 a level, half the ability power and 1.5 times the bonus attack
    // damage.
    let half = Num::ONE / 2;
    let power = scaling(
        Scalar::Int(0),
        10,
        &[("ability_power", half)],
        &[("attack_damage", half * 3)],
    );
    let Param::Scaling(mut power) = power else {
        unreachable!("a scaling table");
    };
    power.base = Ranked::PerRank([100, 200, 300, 400, 500].map(Scalar::Int).to_vec());
    let data = ActionData {
        kind: ActionKind::Cast,
        params: [(DeclaredName::new("power").unwrap(), Param::Scaling(power))].into(),
        ..strike()
    };
    let script = r#"
fn on_resolve(ctx, caster, target) {
    ctx.damage(target, ctx.p.power, "true");
    ctx.add_modifier(target, "mark");
}
"#;
    let ability = game.load("ability", &data, script);
    let caster = game.caster(ability, 2);
    let entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    let parts = (caster_type, Level::new(3).unwrap(), UnitStats::default());
    game.world.entity_mut(entity).insert(parts);
    game.give(caster, "boost");
    let target = game.spawn(1, at(num(5), Num::ZERO, Num::ZERO), Modifiers::default());
    let t = game.world.resource::<SimTick>().start();
    game.cast(caster, ActionTarget::Unit(target));

    // At rank 2 and level 3: 200 + 10 × 2 + 0.5 × 40 + 1.5 × (80 − 60) = 270, which the script
    // deals, 500 → 230, and the mark lasts: 270 ms at 30 ticks a second, 8.1 ticks, up to 9,
    // so it ends as tick t + 10 starts.
    assert!(game.failures().is_empty(), "{:?}", game.failures());
    assert_eq!(game.health(target), 230);
    let mark = Stats::modifier(&game.world, 0, "mark").unwrap();
    let marked = game
        .get_ref::<Modifiers>(target)
        .get(mark, Some(caster))
        .unwrap()
        .until;
    assert_eq!(marked, Some(Tick::new(t.get() + 10)));
}

#[test]
fn a_live_change_follows_its_source_in_the_order_of_the_stats_it_reads() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = Match::with(LIMITS, &declared);
    // Veil: attack damage 53 at level 1. Dual Path gives her spell vamp of 0.06 and 0.00167 a
    // point of bonus attack damage; Fortify gives armor of 10 times her spell vamp, so armor
    // reads spell vamp, which reads attack damage. Armor's place, before spell vamp's, makes the
    // graph's order differ from the places'.
    let veil_type = Units::load_type(
        &mut game.world,
        TypeScope::Mode,
        "veil",
        &UnitTypeData::default(),
    );
    let attack_damage = Stat::named("attack_damage").unwrap();
    let growth = StatValue {
        base: Num::from_int(53).unwrap(),
        per_level: Num::ZERO,
    };
    let growth = StatsData([(attack_damage.clone(), growth)].into());
    let [spell_vamp, armor] = ["spell_vamp", "armor"].map(|name| Stat::named(name).unwrap());
    let mut graph = StatGraph::new(scaling_stats());
    graph.add(&attack_damage, &spell_vamp);
    graph.add(&spell_vamp, &armor);
    let rules: BTreeMap<_, _> = scaling_stats()
        .map(|stat| (stat, StatRule::default()))
        .into();
    let book =
        StatBook::new(&rules, [(veil_type, &growth)], num(6)).with_order(graph.order().unwrap());
    let place = |stat: &Stat| book.named(stat).unwrap().index();
    let places = [&attack_damage, &spell_vamp, &armor].map(place);
    Stats::load_book(&mut game.world, book);
    let vamp = scaling(
        Scalar::Decimal(decimal("0.06")),
        0,
        &[],
        &[("attack_damage", decimal("0.00167"))],
    );
    let dual_path = changing(&[("spell_vamp", param("vamp"))], &[("vamp", vamp)], None);
    let guard = scaling(Scalar::Int(0), 0, &[("spell_vamp", num(10))], &[]);
    let fortify = changing(&[("armor", param("guard"))], &[("guard", guard)], None);
    let boost = changing(&[("attack_damage", int(30))], &[], None);
    for (name, data) in [
        ("boost", boost),
        ("dual_path", dual_path),
        ("fortify", fortify),
    ] {
        Stats::load_modifier(&mut game.world, 0, name, &data, None);
    }
    let veil = game.spawn(
        0,
        at(Num::ZERO, Num::ZERO, Num::ZERO),
        (veil_type, Level::default(), UnitStats::default()),
    );
    game.give(veil, "dual_path");
    game.give(veil, "fortify");
    let values = |game: &Match| {
        let stats = game.get_ref::<UnitStats>(veil).values();
        places.map(|at| stats[at])
    };
    game.world.run_schedule(SimUpdate);
    // 0.06 is 1 006 632.96 bits, to 1 006 633; no bonus; armor 10 times that.
    assert_eq!(
        values(&game),
        [
            num(53),
            Num::from_bits(1_006_633),
            Num::from_bits(10_066_330)
        ]
    );

    // 30 more attack damage: in the same refresh, bonus 30, and 0.00167, 28 017.95 bits to
    // 28 018, times 30 is 840 540: spell vamp 1 847 173 bits, armor ten times it.
    game.give(veil, "boost");
    game.world.run_schedule(SimUpdate);
    assert_eq!(
        values(&game),
        [
            num(83),
            Num::from_bits(1_847_173),
            Num::from_bits(18_471_730)
        ]
    );
}

#[test]
fn a_delivery_hook_reads_its_projectile_and_the_unit_its_cast_aimed_at() {
    let mut game = Match::with(
        LIMITS,
        &[
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Projectiles,
        ],
    );
    let bolt = Units::load_type(
        &mut game.world,
        TypeScope::Mode,
        "bolt",
        &UnitTypeData::default(),
    );
    let data = ProjectileData {
        speed: num(15),
        width: Num::ZERO,
        range: None,
        homing: false,
        stop_on_hit: true,
        once_per_cast: false,
        hits: None,
        gravity: None,
        sight_radius: None,
        collide: None,
    };
    Projectiles::load_type(&mut game.world, bolt, &data);
    let shot = ActionData {
        delivery: Some(DeliveryData::Projectile {
            unit_type: DeclaredName::new("bolt").unwrap(),
            count: NonZeroU8::MIN,
            spread_deg: Num::ZERO,
        }),
        ..strike()
    };
    // Each hook deals damage only when it reads the bolt still there, and the hit's target is the
    // unit the cast aimed at.
    let source = r#"
        fn on_hit(ctx, caster, target, hit) {
            if hit.delivery.unit_type == "bolt" && hit.target == target {
                ctx.damage(target, 50, "true");
            }
        }
        fn on_end(ctx, caster, hit) {
            if hit.delivery.unit_type == "bolt" && hit.target != () {
                ctx.damage(caster, 7, "true");
            }
        }
    "#;
    let ability = game.load("shot", &shot, source);
    let caster = game.caster(ability, 1);
    let target = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), ());
    // The damage the hook deals carries the bolt's hit, which the target's `watch` reads, and
    // answers with 1 damage to its source: the hit's target, the unit the cast aimed at, after
    // 3 m flown.
    game.load_stats();
    let watch = r#"
        fn on_damage_taken(ctx, m, d) {
            if d.hit == () || d.hit.target != m.carrier || d.hit.distance != 3 {
                throw "the damage carries the hit of its bolt";
            }
            ctx.damage(d.source, 1, "true");
        }
    "#;
    let script = Units::compile(&mut game.world, watch).unwrap();
    Stats::load_modifier(
        &mut game.world,
        0,
        "watch",
        &scripted(None, &[]),
        Some(script),
    );
    game.give(target, "watch");

    // The bolt launches in tick 0 and flies half a meter a tick from tick 1: it reaches the
    // target 3 m out in tick 6, hits it, and ends there, as it stops on a hit.
    game.cast(caster, ActionTarget::Unit(target));
    game.run_until(7);
    assert_eq!((game.health(target), game.health(caster)), (450, 492));
    assert!(game.failures().is_empty(), "{:?}", game.failures());
}

#[test]
fn a_script_launches_a_projectile_only_in_the_form_its_type_flies() {
    // Each cast launches its own projectile in tick 0, and its `on_resolve` one more: at the
    // target, or along the direction to it. A homing type flies only at a unit, a line type only
    // along a direction; the other form fails the call, and a failed cast launches nothing.
    let forms = [
        ("ctx.projectile(caster.pos, target)", true),
        (
            "ctx.projectile(caster.pos, caster.pos.direction_to(target.pos))",
            false,
        ),
    ];
    for homing in [false, true] {
        for (call, at_unit) in forms {
            let mut game = Match::with(
                LIMITS,
                &[
                    Capability::Stats,
                    Capability::Combat,
                    Capability::Abilities,
                    Capability::Projectiles,
                ],
            );
            let bolt = Units::load_type(
                &mut game.world,
                TypeScope::Mode,
                "bolt",
                &UnitTypeData::default(),
            );
            let data = ProjectileData {
                speed: num(15),
                width: Num::ZERO,
                range: None,
                homing,
                stop_on_hit: true,
                once_per_cast: false,
                hits: None,
                gravity: None,
                sight_radius: None,
                collide: None,
            };
            Projectiles::load_type(&mut game.world, bolt, &data);
            let shot = ActionData {
                delivery: Some(DeliveryData::Projectile {
                    unit_type: DeclaredName::new("bolt").unwrap(),
                    count: NonZeroU8::MIN,
                    spread_deg: Num::ZERO,
                }),
                ..strike()
            };
            let source = format!("fn on_resolve(ctx, caster, target) {{ {call}; }}");
            let ability = game.load("shot", &shot, &source);
            let caster = game.caster(ability, 1);
            let target = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), ());
            game.cast(caster, ActionTarget::Unit(target));
            game.run_until(1);
            let mut projectiles = game.world.query::<&Projectile>();
            let launched = projectiles.iter(&game.world).count();
            let failed: Vec<_> = game
                .failures()
                .iter()
                .map(|failure| match failure.error {
                    CallError::Api(error) => Some(error),
                    _ => None,
                })
                .collect();
            if homing == at_unit {
                assert_eq!((launched, failed), (2, vec![]), "{homing} {call}");
            } else {
                let other = Some(ApiError::OtherFlight);
                assert_eq!((launched, failed), (0, vec![other]), "{homing} {call}");
            }
        }
    }
}

/// Fan of Frost as data alone: five arrows of `frost_arrow` over 30 degrees, aimed along a
/// direction, whose hits deal the `damage` param, 30, as physical damage, apply `chilled`, and
/// restore 2 mana to the caster.
fn fan_of_frost() -> ActionData {
    let effect = |does| EffectData {
        does,
        to: EffectTo::Reached,
    };
    ActionData {
        script: None,
        targeting: Targeting::Direction,
        range: None,
        delivery: Some(DeliveryData::Projectile {
            unit_type: DeclaredName::new("frost_arrow").unwrap(),
            count: NonZeroU8::new(5).unwrap(),
            spread_deg: num(30),
        }),
        on_hit: vec![
            effect(Effecting::Damage {
                amount: Number::Param(ParamRef {
                    param: DeclaredName::new("damage").unwrap(),
                }),
                kind: DeclaredName::new("physical").unwrap(),
            }),
            effect(Effecting::Modifier {
                id: DeclaredName::new("chilled").unwrap(),
                duration_ms: None,
            }),
            EffectData {
                does: Effecting::Restore {
                    pool: DeclaredName::new("mana").unwrap(),
                    amount: Number::Value(Scalar::Int(2)),
                },
                to: EffectTo::Source,
            },
        ],
        params: BTreeMap::from([(
            DeclaredName::new("damage").unwrap(),
            Param::Ranked(Ranked::One(Scalar::Int(30))),
        )]),
        ..strike()
    }
}

#[test]
fn fan_of_frost_from_data_alone_hits_exactly_the_units_in_reach() {
    // Rime's Fan of Frost as design 04 writes it, with no script: five arrows of 0.5 m wide,
    // fanned over 30 degrees around the aim along +x, each for 6 m at half a meter a tick, whose
    // hits deal the `damage` param, 30, and apply `chilled`; and each hit restores 2 mana to the
    // caster.
    let mut game = Match::with(
        LIMITS,
        &[
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Projectiles,
        ],
    );
    game.load_stats();
    let chilled = ModifierData {
        duration_ms: Some(int(2000)),
        ..scripted(None, &[])
    };
    let chilled = ModifierData {
        script: None,
        ..chilled
    };
    Stats::load_modifier(&mut game.world, 0, "chilled", &chilled, None);
    let arrow = Units::load_type(
        &mut game.world,
        TypeScope::Mode,
        "frost_arrow",
        &UnitTypeData::default(),
    );
    let data = ProjectileData {
        speed: num(15),
        width: halves(1),
        range: Some(num(6)),
        homing: false,
        stop_on_hit: false,
        once_per_cast: false,
        hits: None,
        gravity: None,
        sight_radius: None,
        collide: None,
    };
    Projectiles::load_type(&mut game.world, arrow, &data);
    let fan = fan_of_frost();
    let ability = Actions::load(&mut game.world, 0, "fan_of_frost", &fan, None, 1).unwrap();
    Abilities::load_effects(&mut game.world, ability, 0, &fan);
    let caster = game.caster(ability, 1);
    // Bodiless units: on the middle arrow 4 m out; on the outer arrow 4 m out, at 15 degrees,
    // (3.8637, 1.0353); between two arrows at 4 m, 3.75 degrees off each, 0.26 m from each line,
    // past its 0.25; at 26.6 degrees, past the fan; 8 m out on the middle line, past the range;
    // behind the caster; and an ally of the caster's on the middle line.
    let point = |x: Num, z: Num| at(x, Num::ZERO, z);
    let decimal = |text: &str| text.parse::<Num>().unwrap();
    let enemies = [
        point(num(4), Num::ZERO),
        point(decimal("3.8637"), decimal("1.0353")),
        point(decimal("3.9914"), decimal("0.2617")),
        point(num(4), num(2)),
        point(num(8), Num::ZERO),
        point(num(-2), Num::ZERO),
    ]
    .map(|pos| game.spawn(1, pos, ()));
    let ally = game.spawn(0, point(num(2), Num::ZERO), ());
    for unit in enemies.iter().chain([&ally]) {
        let entity = game.world.resource::<EntityIndex>().get(*unit).unwrap();
        game.world.entity_mut(entity).insert(Modifiers::default());
    }
    game.cast(caster, ActionTarget::Point(point(num(10), Num::ZERO)));
    game.run_until(16);
    let chill = Stats::modifier(&game.world, 0, "chilled").unwrap();
    let struck = |unit: StableId| {
        let entity = game.world.resource::<EntityIndex>().get(unit).unwrap();
        let chilled = game
            .world
            .get::<Modifiers>(entity)
            .unwrap()
            .get(chill, Some(caster));
        (game.health(unit), chilled.is_some())
    };
    let reached = enemies.map(struck);
    assert_eq!(
        reached,
        [
            (470, true),
            (470, true),
            (500, false),
            (500, false),
            (500, false),
            (500, false),
        ]
    );
    assert_eq!(struck(ally), (500, false));
    // The caster paid its 10 mana of 100, and two hits gave back 2 each.
    assert_eq!(game.pool(caster), 94);
    assert!(game.failures().is_empty(), "{:?}", game.failures());
}

#[test]
fn an_area_reaches_the_bodies_within_its_radius_once_at_its_delay_and_ends() {
    // A delay of 0 ms triggers in tick 1, of 100 ms, 3 ticks at 30 a second, in tick 3: an area
    // lands in the tick its cast resolves, tick 0, and triggers its delay later, one tick at the
    // least.
    for (delay_ms, trigger) in [(0, 1), (100, 3)] {
        let declared = [
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Areas,
        ];
        let mut game = Match::with(LIMITS, &declared);
        let blast = Units::load_type(
            &mut game.world,
            TypeScope::Mode,
            "blast",
            &UnitTypeData::default(),
        );
        let data = AreaData {
            radius: num(2),
            delay_ms,
            duration_ms: 0,
            affects: None,
            inside: AreaInside::default(),
        };
        Areas::load_type(&mut game.world, blast, 0, &data);
        let shot = ActionData {
            targeting: Targeting::Point,
            delivery: Some(DeliveryData::Area {
                unit_type: DeclaredName::new("blast").unwrap(),
            }),
            ..strike()
        };
        // Each hook deals damage only when it reads the area still there, and no aimed unit.
        let source = r#"
            fn on_hit(ctx, caster, target, hit) {
                if hit.delivery.unit_type == "blast" && hit.target == () {
                    ctx.damage(target, 50, "true");
                }
            }
            fn on_end(ctx, caster, hit) {
                if hit.delivery.unit_type == "blast" {
                    ctx.damage(caster, 7, "true");
                }
            }
        "#;
        let ability = game.load("shot", &shot, source);
        let caster = game.caster(ability, 1);
        // The area lands on (4, 0, 0), of radius 2: a body of no radius 2 m away is inside it and
        // one a bit farther is not; a body of radius 0.5 2.5 m away touches it; an enemy whose
        // tags block it as a target is reached all the same; an ally is not.
        let center = num(4);
        let edge = game.spawn(1, at(center + num(2), Num::ZERO, Num::ZERO), ());
        let beyond = game.spawn(1, at(center, Num::ZERO, num(2) + Num::EPSILON), ());
        let body = Body::new(halves(1)).unwrap();
        let wide = game.spawn(1, at(center - halves(5), Num::ZERO, Num::ZERO), body);
        let hidden = game.spawn(1, at(center, Num::ZERO, Num::ONE), ());
        game.set_blocks(hidden, &[Block::Target]);
        let ally = game.spawn(0, at(center, Num::ZERO, Num::ZERO), ());
        let units = [edge, beyond, wide, hidden, ally, caster];
        let areas = |game: &mut Match| {
            let mut query = game.world.query::<&Area>();
            query.iter(&game.world).count()
        };

        game.cast(
            caster,
            ActionTarget::Point(at(center, Num::ZERO, Num::ZERO)),
        );
        game.run_until(trigger);
        assert_eq!(units.map(|unit| game.health(unit)), [500; 6], "{delay_ms}");
        assert_eq!(areas(&mut game), 1);
        // It triggers, deals 50 to each enemy it reaches, ends at once with no duration, and
        // its `on_end` deals 7 to its caster.
        game.run_until(trigger + 1);
        let healths = units.map(|unit| game.health(unit));
        assert_eq!(healths, [450, 500, 450, 450, 500, 493], "{delay_ms}");
        assert_eq!(areas(&mut game), 0);
        assert!(game.failures().is_empty(), "{:?}", game.failures());
    }
}

#[test]
fn a_cast_under_way_ends_when_its_caster_dies() {
    // Strike with a windup of 200 ms, 6 ticks: cast in tick 0, it would resolve in tick 6. The
    // caster dies in tick 2 and comes back in tick 4, before the cast's time, and the cast is
    // gone: the enemy takes nothing, then or later.
    let mut game = Match::new();
    let windup = ActionData {
        windup_ms: Some(Ranked::One(int(200))),
        ..strike()
    };
    let strike = game.load("strike", &windup, STRIKE);
    let caster = game.caster(strike, 1);
    let enemy = game.spawn(1, at(num(3), Num::ZERO, Num::ZERO), ());
    game.cast(caster, ActionTarget::Unit(enemy));
    assert!(game.casting(caster).is_some());
    game.run_until(2);
    let entity = game.world.resource::<EntityIndex>().get(caster).unwrap();
    game.world
        .get_mut::<Pools>(entity)
        .unwrap()
        .take(PoolId::FIRST, num(500));
    game.run_until(3);
    assert!(game.world.entity(entity).contains::<Dead>());
    assert_eq!(game.casting(caster), None);
    game.world.entity_mut(entity).remove::<Dead>();
    game.give_pools(caster, 100, 20);
    game.run_until(12);
    assert_eq!(game.health(enemy), 500);
    assert_eq!(game.pool(caster), 100);
}
