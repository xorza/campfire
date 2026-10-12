use std::collections::BTreeMap;
use std::num::NonZeroU8;

use bevy_ecs::bundle::Bundle;
use bevy_ecs::world::Mut;
use campfire_common::{PlayerSlot, StateHash, Tick};
use campfire_math::{Num, Rng, RngStream, Vec3};
use campfire_script::NumError;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{EntityIndex, Position, SimRng, SimTick, SimWorld, StableId, StateCopy};

use super::*;
use crate::abilities::test_world::{MANA, RAGE, scaling_stats};
use crate::actions::Actions;
use crate::actions::action_book::ActionBook;
use crate::actions::action_call::Started;
use crate::actions::action_data::{ActionData, ChannelData, ChargeUpData, ChargesData, Toggle};
use crate::actions::action_data_field::ActionDataField;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_range::ActionRange;
use crate::actions::action_slots::ActionSlots;
use crate::actions::cost_target::CostTarget;
use crate::actions::delivery_data::DeliveryData;
use crate::actions::effect_data::{
    DamageFields, DoesData, EffectData, EffectTo, LaunchFields, ModifierFields, MoveData,
    PurgeFields, RestoreFields, XpFields,
};
use crate::actions::error::ActionField;
use crate::actions::in_progress::{InProgress, OrderPhase};
use crate::actions::kind_data::KindData;
use crate::actions::range_field::RangeField;
use crate::actions::slot_aim::SlotAim;
use crate::actions::slot_kind_data::{SlotKindData, SlotRanks};
use crate::actions::slot_kinds::SlotKinds;
use crate::actions::slot_times::SlotTimes;
use crate::actions::targeting::Targeting;
use crate::areas::Areas;
use crate::areas::area::Area;
use crate::areas::area_data::{AreaData, AreaInsideData};
use crate::combat::Combat;
use crate::combat::assist_window::AssistWindow;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::on_death::OnDeath;
use crate::combat::pass_queue::PassQueue;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::test_world::Armed;
use crate::geometry::body_box::BodyBox;
use crate::navigation::Navigation;
use crate::navigation::destination::Destination;
use crate::orders::Orders;
use crate::orders::ai_data::AiData;
use crate::orders::order::{Action, Order};
use crate::players::Players;
use crate::players::players_world::{PlayersWorld, PlayersWorldCrate};
use crate::players::resource_id::ResourceId;
use crate::progression::Progression;
use crate::progression::experience::Experience;
use crate::progression::points::Points;
use crate::progression::track_data::{Thresholds, TrackData};
use crate::progression::track_set::TrackSet;
use crate::projectiles::Projectiles;
use crate::projectiles::projectile::Projectile;
use crate::projectiles::projectile_data::ProjectileData;
use crate::release::Release;
use crate::release::r#match::Match;
use crate::scripts::error::ApiError;
use crate::scripts::error::internals::FailureKind;
use crate::scripts::hook::Hook;
use crate::scripts::name_kind::NameKind;
use crate::scripts::pool::Pool;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::internals::FailedCall;
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::state_decl::synced_state_decl::{SyncTo, SyncedStateDecl};
use crate::scripts::state_decl::{StateDecl, StateDefault, StateType};
use crate::scripts::state_value::StateValue;
use crate::stats::Stats;
use crate::stats::level::Level;
use crate::stats::life::Life;
use crate::stats::lifetime::Hold;
use crate::stats::live_carriers::LiveCarriers;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_data::ModifierData;
use crate::stats::modifiers::Modifiers;
use crate::stats::pool_id::PoolId;
use crate::stats::pools::Pools;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_change::StatChange;
use crate::stats::stat_graph::StatGraph;
use crate::stats::stat_op::StatOp;
use crate::stats::stat_rule::StatRule;
use crate::stats::stats_data::{StatValue, StatsData};
use crate::stats::unit_stats::UnitStats;
use crate::units::Units;
use crate::units::action_id::ActionId;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::ForcedMove;
use crate::units::move_step::MoveStep;
use crate::units::owner::Owner;
use crate::units::tag_data::TagData;
use crate::units::team::Team;
use crate::units::track_id::TrackId;
use crate::units::type_scope::TypeScope;
use crate::units::unit_state::{SyncNone, UnitState};
use crate::units::unit_type::UnitType;
use crate::units::unit_type_core::UnitTypeCore;
use crate::values::action_start::ActionStart;
use crate::values::damage_kind::DamageKind;
use crate::values::declared_name::DeclaredName;
use crate::values::error::TimeTooLarge;
use crate::values::filter_data::FilterData;
use crate::values::number::{Number, ParamRef};
use crate::values::package_path::PackagePath;
use crate::values::param::{Param, Scaling};
use crate::values::rank::Rank;
use crate::values::ranked::Ranked;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;

/// Lash Out as the MOBA's Husk had it when these tests were written: the engine's tests keep
/// their own copy, so a balance change to the MOBA's hero changes none of them.
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
/// Halves of a meter.
fn halves(value: i64) -> Num {
    Num::from_bits(value << (Num::FRAC_BITS - 1))
}

/// A unit that strikes for `damage` within 2 m, as its windup of no ticks ends, every 5 ticks.
fn striker(damage: i64) -> Armed {
    Armed::melee(Num::int(500), Num::int(2), 0, 5, Num::int(damage)).on_death(OnDeath::Stay)
}

/// The match's pools, by name in order: the life pool first, as it is until a mode binds one.
const POOLS: [&str; 3] = ["health", "mana", "rage"];

/// A cost of `amount` in pool `name`.
fn cost(name: &str, amount: Number) -> BTreeMap<DeclaredName, Ranked<Number>> {
    BTreeMap::from([(DeclaredName::new(name).unwrap(), Ranked::One(amount))])
}

/// Husk's Lash Out as its data declares it: no target, a cooldown of 10 s down to 6 s, 35 of the
/// caster's mana, and damage within 3.5 m of 75 to 175, plus half the caster's ability power.
fn lash_out() -> ActionData {
    ActionData {
        script: Some(PackagePath::parse("scripts/lash_out.rhai").unwrap()),
        cooldown_ms: Some(Ranked::PerRank(
            [10_000, 9000, 8000, 7000, 6000].map(int).to_vec(),
        )),
        cost: cost("mana", int(35)),
        params: BTreeMap::from([
            (
                DeclaredName::new("radius").unwrap(),
                Param::Ranked(Ranked::One(Scalar::Decimal(halves(7)))),
            ),
            (
                DeclaredName::new("damage").unwrap(),
                Param::Scaling(Scaling {
                    base: Ranked::PerRank([75, 100, 125, 150, 175].map(Num::int).to_vec()),
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
        ..ActionData::cast(Targeting::None)
    }
}

/// A unit-targeted ability: `damage` true damage to an enemy within 5 m, every second, for 10
/// mana and 4 rage.
fn strike() -> ActionData {
    ActionData {
        script: Some(PackagePath::parse("strike.rhai").unwrap()),
        range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
            Num::int(5),
        )))),
        cooldown_ms: Some(Ranked::One(int(1001))),
        cost: BTreeMap::from([
            (DeclaredName::new("mana").unwrap(), Ranked::One(int(10))),
            (DeclaredName::new("rage").unwrap(), Ranked::One(int(4))),
        ]),
        params: BTreeMap::from([(
            DeclaredName::new("damage").unwrap(),
            Param::Ranked(Ranked::One(Scalar::Int(50))),
        )]),
        ..ActionData::cast(Targeting::Unit(FilterData::parse("enemies").unwrap()))
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
    // With no script or cooldown, an attack lacks its weapon's rate, then a cast refuses the
    // rate. An attack takes the params and an `on_hit` list, and refuses an `on_end` list.
    action.script = None;
    action.cooldown_ms = None;
    assert_eq!(
        ActionDataField::misused(&action, ActionKind::Attack),
        Some(ActionDataField::Rate)
    );
    action.rate = Some(Stat::named("armor").unwrap());
    action.damage = Some(Stat::named("attack_damage").unwrap());
    action.damage_kind = Some(DeclaredName::new("physical").unwrap());
    action.on_hit = sapper(false).on_hit;
    assert_eq!(ActionDataField::misused(&action, ActionKind::Attack), None);
    // Its kind's fields come as the attack needs them; as a cast, the rate it refuses is the
    // error.
    let attack = ActionData {
        kind: ActionKind::Attack,
        ..action.clone()
    };
    assert_eq!(
        attack.kind_data(),
        Ok(KindData::Attack {
            rate: &Stat::named("armor").unwrap(),
            damage: &Stat::named("attack_damage").unwrap(),
            damage_kind: &DeclaredName::new("physical").unwrap(),
        })
    );
    let cast = ActionData {
        kind: ActionKind::Cast,
        ..action.clone()
    };
    assert_eq!(cast.kind_data(), Err(ActionDataField::Rate));
    let ending = ActionData {
        on_end: action.on_hit.clone(),
        ..action.clone()
    };
    assert_eq!(
        ActionDataField::misused(&ending, ActionKind::Attack),
        Some(ActionDataField::OnEnd)
    );
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
    // Given its unit type, and none of the params and the hit list a train refuses, it gives
    // the train that type.
    let train = ActionData {
        kind: ActionKind::Train,
        unit_type: Some(DeclaredName::new("grunt").unwrap()),
        params: BTreeMap::new(),
        on_hit: Vec::new(),
        ..train
    };
    let grunt = DeclaredName::new("grunt").unwrap();
    assert_eq!(train.kind_data(), Ok(KindData::Train { unit_type: &grunt }));
}

const STRIKE: &str =
    r#"fn on_resolve(ctx, caster, target) { ctx.damage(target, ctx.p.damage, "true"); }"#;

fn abilities_match() -> Match {
    abilities_match_with(
        ScriptLimits::ROOMY,
        &[Capability::Stats, Capability::Combat, Capability::Abilities],
    )
}

/// A match of two players of `declared`, whose scripts run within `limits`, and who hold
/// gold, as a mode would keep it.
fn abilities_match_with(limits: ScriptLimits, declared: &[Capability]) -> Match {
    let mut game = Match::server(declared, ScriptBudgets::new(limits, 2));
    // The damage kinds a mode would declare: the MOBA test content's.
    Units::name_kinds(&game, &["physical", "magic", "true"], &POOLS, &["gold"]);
    Players::spawn(&mut game, 2, 1);
    game
}

/// A match of stats, combat and abilities whose modifiers, by name in package 0, run
/// `source`, as `scripted` declares them.
fn abilities_match_with_modifiers(source: &str, modifiers: &[(&str, ModifierData)]) -> Match {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    let script = Units::compile_hooked(&mut game, source).unwrap();
    let mut sorted = modifiers.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    for (name, data) in &sorted {
        Stats::load_modifier(&mut game, 0, name, data, Some(script));
    }
    game
}

#[test]
fn damage_of_a_kind_the_mode_does_not_declare_fails_the_cast() {
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let fire = Abilities::load(
        &mut game,
        "fire",
        &lash_out(),
        &LASH_OUT.replace(r#""magic""#, r#""fire""#),
    );
    let husk = Abilities::caster(&mut game, fire, 2);
    let near = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
    Abilities::cast(&mut game, husk, ActionTarget::None);
    // The call fails, so the cast applies nothing: no damage, no cost.
    assert_eq!(
        (Stats::health(&game, near), Abilities::pool(&game, husk)),
        (500, 100)
    );
    let failed = FailedCall {
        id: Some(husk),
        hook: Hook::OnResolve,
        kind: FailureKind::Api(ApiError::Unknown(NameKind::DamageKind)),
    };
    assert_eq!(Abilities::failed_calls(&game), [failed]);
}

#[test]
fn lash_out_hits_every_enemy_within_its_radius_exactly() {
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    // Strike's one param name comes first in the frame, so Lash Out's three follow from the
    // second.
    Abilities::load(&mut game, "strike", &strike(), STRIKE);
    let lash_out = Abilities::load(&mut game, "lash_out", &lash_out(), LASH_OUT);
    let husk = Abilities::caster(&mut game, lash_out, 2);
    let near = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
    // At exactly 3.5 m: within the radius.
    let edge = Abilities::spawn_combatant(
        &mut game,
        Position::try_from(Vec3::from((0, 0, halves(7)))).unwrap(),
        Team::new(1),
        (),
    );
    let beyond = Abilities::spawn_combatant(&mut game, (4, 0, 0), Team::new(1), ());
    // Up at y = 9, 2 m away on the ground plane.
    let high = Abilities::spawn_combatant(&mut game, (0, 9, 2), Team::new(1), ());
    let ally = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(0), ());
    let dead = Abilities::spawn_combatant(&mut game, (1, 0, 1), Team::new(1), ());
    game.insert_parts(dead, Dead);

    // Rank 2 deals 100, and 0.5 × 0 ability power: 500 → 400 for the three enemies in reach.
    // It costs 35 of 100, and its 9000 ms cooldown is 9 × 30 = 270 ticks.
    Abilities::cast(&mut game, husk, ActionTarget::None);
    let healths =
        |game: &Match| [near, edge, beyond, high, ally, dead].map(|id| Stats::health(game, id));
    assert_eq!(healths(&game), [400, 400, 500, 400, 500, 500]);
    assert_eq!(Abilities::pool(&game, husk), 65);
    assert_eq!(Abilities::time(&game, husk).ready_at, Tick::new(270));
    assert_eq!(Abilities::failed_calls(&game), []);

    // On cooldown until tick 270: a cast in tick 1 does nothing.
    Abilities::cast(&mut game, husk, ActionTarget::None);
    assert_eq!(healths(&game), [400, 400, 500, 400, 500, 500]);
    assert_eq!(Abilities::pool(&game, husk), 65);
    // An ability that takes no target ignores the one its order names: the cast at the ally hits
    // the same three enemies.
    game.run_until(270);
    Abilities::cast(&mut game, husk, ActionTarget::Unit(ally));
    assert_eq!(healths(&game), [300, 300, 500, 300, 500, 500]);
    assert_eq!(Abilities::pool(&game, husk), 30);
    // 30 left cannot pay 35.
    game.run_until(540);
    Abilities::cast(&mut game, husk, ActionTarget::None);
    assert_eq!(healths(&game), [300, 300, 500, 300, 500, 500]);
    assert_eq!(Abilities::pool(&game, husk), 30);
}

#[test]
fn a_knock_back_interrupts_a_windup_and_the_cast_waits_for_its_end() {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
    ];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    // Aim winds up 300 ms, 9 ticks, within 10 m; Shove, at once, knocks its target 3 m away from
    // its caster over 100 ms, 3 ticks.
    let aim = ActionData {
        windup_ms: Some(Ranked::One(int(300))),
        range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
            Num::int(10),
        )))),
        ..strike()
    };
    let aim = Abilities::load(&mut game, "aim", &aim, STRIKE);
    let shove = ActionData {
        script: Some(PackagePath::parse("shove.rhai").unwrap()),
        range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
            Num::int(5),
        )))),
        ..ActionData::cast(Targeting::Unit(FilterData::parse("enemies").unwrap()))
    };
    let script =
        "fn on_resolve(ctx, caster, target) { ctx.knock_back(target, caster.pos, 3, 100); }";
    let shove = Abilities::load(&mut game, "shove", &shove, script);
    let caster = Abilities::caster(&mut game, aim, 1);
    game.insert_parts(caster, Navigation::walker(MoveStep::new(Num::ONE).unwrap()));
    let slots = ActionSlots::new([(shove, SlotKind::new(0), Rank::new(1))]).timed();
    let shover = Abilities::spawn_combatant(&mut game, (5, 0, 0), Team::new(1), slots);

    // Aim starts in tick 0, to resolve in tick 9. Shove resolves in tick 2's Hit stage, after
    // Move, and knocks the caster along −x in ticks 3, 4 and 5: Aim's windup is interrupted in
    // tick 3, and its order waits through tick 5. It starts again in tick 6, from (−3, 0, 0),
    // and resolves in tick 15.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(shover));
    game.step();
    Abilities::cast(&mut game, shover, ActionTarget::Unit(caster));
    game.run_until(6);
    assert_eq!(*game.part::<Position>(caster), Position::from((-3, 0, 0)));
    let ordered = InProgress::Order {
        aim: SlotAim {
            slot: 0,
            target: ActionTarget::Unit(shover),
        },
        phase: OrderPhase::Ordered,
    };
    assert_eq!(
        game.part::<ActionSlots>(caster).in_progress(),
        Some(ordered)
    );
    game.run_until(15);
    assert_eq!(Stats::health(&game, shover), 500);
    game.step();
    assert_eq!(Stats::health(&game, shover), 450);
    assert_eq!(Abilities::failed_calls(&game), []);

    // A forced move of a unit that does not walk, a dash of no step, a knock back of no
    // distance or of no time, fails.
    let refused = [
        (shover, "ctx.teleport(of, of.pos)", ApiError::NoWalker),
        (caster, "ctx.dash(of, of.pos, 0)", ApiError::NotASpeed),
        (caster, "ctx.dash(of, of.pos, -1)", ApiError::NotASpeed),
        (
            caster,
            "ctx.knock_back(of, of.pos, 0, 100)",
            ApiError::NotADistance,
        ),
        (
            caster,
            "ctx.knock_back(of, of.pos, 1, 0)",
            ApiError::ZeroTime,
        ),
        (
            caster,
            "ctx.knock_back(of, of.pos, 1, -1)",
            ApiError::NegativeTime,
        ),
    ];
    for (of, call, error) in refused {
        let failed = Units::read(&mut game, call, of).unwrap_err();
        assert_eq!(failed.kind(), FailureKind::Api(error), "{call}");
    }
}

#[test]
fn a_listed_move_knocks_back_and_dashes_as_the_calls_do_and_the_dash_delivers_the_cast() {
    let enemy = Team::new(1);
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
    ];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    // Lunge knocks the unit it reaches 3 m away from its caster over 100 ms, 3 ticks, and dashes
    // its caster at it, 30 m a second, a meter a tick. It delivers at once, so the dash is its
    // delivery: as the dash ends, its `on_end` list deals its caster 1, and its hook deals the
    // dash's target the meters it went and ten times how far from where it began it ended, when
    // no projectile or area delivered it and its last step went somewhere.
    let knock_back = MoveData::KnockBack {
        from: EffectTo::Source,
        distance: int(3),
        ms: int(100),
    };
    let dash = MoveData::Dash {
        to: EffectTo::Reached,
        speed: int(30),
    };
    let damage = DoesData::Damage(DamageFields {
        amount: int(1),
        kind: DeclaredName::new("true").unwrap(),
    });
    let lunge = ActionData {
        script: Some(PackagePath::parse("lunge.rhai").unwrap()),
        range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
            Num::int(10),
        )))),
        on_resolve: vec![
            effect(DoesData::Move(knock_back), EffectTo::Reached),
            effect(DoesData::Move(dash), EffectTo::Source),
        ],
        on_end: vec![effect(damage, EffectTo::Source)],
        ..ActionData::cast(Targeting::Unit(FilterData::parse("enemies").unwrap()))
    };
    let source = r#"
        fn on_end(ctx, caster, hit) {
            if hit.delivery == () && hit.direction != () {
                let from = hit.pos.distance_to(ctx.origin);
                ctx.damage(hit.target, hit.distance + from * 10, "true");
            }
        }
    "#;
    let script = Units::compile_hooked(&mut game, source).unwrap();
    let id = Actions::load(&mut game, 0, "lunge", &lunge, Some(script), 1).unwrap();
    Actions::load_effect_lists(&mut game, id, 0, &lunge);
    let lunge = id;
    let walker = || Navigation::walker(MoveStep::new(Num::ONE).unwrap());
    let caster = Abilities::caster(&mut game, lunge, 1);
    game.insert_parts(caster, walker());
    let target = Abilities::spawn_combatant(&mut game, (2, 0, 0), enemy, walker());
    // It resolves in tick 0. The caster, of the lower id, moves first each tick, at the target's
    // place before the target's own move: 1, 2, 3, then 4, and in tick 5 onto 5, where they
    // touch, as neither has a body. The target goes 3, 4, 5.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
    let mut places = Vec::new();
    let mut healths = Vec::new();
    for _ in 0..5 {
        game.step();
        let x = |id| game.part::<Position>(id).get().x.to_int().unwrap();
        places.push((x(caster), x(target)));
        healths.push((Stats::health(&game, caster), Stats::health(&game, target)));
    }
    assert_eq!(places, [(1, 3), (2, 4), (3, 5), (4, 5), (5, 5)]);
    // The dash ends in tick 5, on (5, 0, 0), after 5 steps of a meter from the origin: its
    // caster takes 1, and its target 5 + 50 = 55, of 500. No other tick runs its end.
    assert_eq!(
        healths,
        [(500, 500), (500, 500), (500, 500), (500, 500), (499, 445)]
    );
    assert_eq!(Abilities::failed_calls(&game), []);
    // A unit that does not walk takes no forced move: the list fails the cast.
    let still = Abilities::spawn_combatant(&mut game, (6, 0, 0), enemy, ());
    game.run_until(40);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(still));
    let failed = Abilities::failed_calls(&game);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].kind, FailureKind::Api(ApiError::NoWalker));
    // A dash a knock back cuts delivers nothing, as League of Legends' and Dota 2's cut dashes
    // deliver nothing: the knock back replaces it, and its end runs no `on_end`.
    let far = Abilities::spawn_combatant(&mut game, (12, 0, 0), enemy, walker());
    game.run_until(80);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(far));
    let dashing = game.part::<ForcedMove>(caster);
    assert!(matches!(
        dashing,
        ForcedMove::Dash {
            delivers: Some(_),
            ..
        }
    ));
    let cut = ForcedMove::KnockBack {
        to: Vec3::new(Num::ZERO, Num::ZERO, Num::ZERO),
        left: 2,
    };
    game.insert_parts(caster, cut);
    game.run_until(120);
    assert_eq!(game.part::<Position>(caster).get().x, Num::ZERO);
    assert_eq!(
        (Stats::health(&game, caster), Stats::health(&game, far)),
        (499, 500)
    );
    assert_eq!(Abilities::failed_calls(&game), []);
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
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    // Eleven units whose AI spins, all due in every tick: ten calls fail at the 10 000 limit and
    // spend the 100 000 of the think pool, and the eleventh finds it spent. Their type loads
    // before any unit spawns, as a match's types do.
    let spinner = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "spinner",
        &UnitTypeCore::default(),
    );
    let ai = AiData {
        ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
        think_ms: 1,
    };
    let spin = "fn on_think(ctx, unit) { loop {} }";
    let spin = Units::compile_hooked(&mut game, spin).unwrap();
    Orders::load_ai(&mut game, spinner, &ai, spin).unwrap();
    let strike = Abilities::load(&mut game, "strike", &strike(), STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy = Abilities::spawn_combatant(&mut game, (5, 0, 0), Team::new(1), ());
    let spinners: Vec<_> = (0..11)
        .map(|z| {
            Abilities::spawn_combatant_of(&mut game, spinner, (0, 0, 20 + z), Team::new(2), ())
        })
        .collect();

    // The cast in the same tick draws from its player's pool, whole: 500 → 450, 100 → 90.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
    assert_eq!(Stats::health(&game, enemy), 450);
    assert_eq!(Abilities::pool(&game, caster), 90);
    // The think pool runs out after 10 of the spinning calls: the first 10 spinners, by stable
    // id, spend it, and the last stays due.
    let expected: Vec<_> = spinners[..10]
        .iter()
        .map(|&id| FailedCall {
            id: Some(id),
            hook: Hook::OnThink,
            kind: FailureKind::CallLimit,
        })
        .collect();
    assert_eq!(Abilities::failed_calls(&game), expected);
    let mut budgets = game.resource_mut::<ScriptBudgets>();
    assert_eq!(budgets.get_mut(Pool::Think).left(), 0);
}

#[test]
fn a_cast_passes_its_checks_or_does_nothing() {
    let mut game = abilities_match();
    let strike = Abilities::load(&mut game, "strike", &strike(), STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let mut caster_at = |x: i16, rank: u8, mana: i64, rage: i64| {
        let id = Abilities::spawn_combatant(
            &mut game,
            (x, 0, 0),
            Team::new(0),
            (
                Owner::new(PlayerSlot::new(0)),
                ActionSlots::new([(strike, SlotKind::new(0), Rank::new(rank))]).timed(),
            ),
        );
        Abilities::give_pools(&mut game, id, mana, rage);
        id
    };
    let unlearned = caster_at(1, 0, 100, 20);
    let poor = caster_at(2, 1, 5, 20);
    let calm = caster_at(2, 1, 100, 3);
    let spent = caster_at(2, 1, 100, 20);
    let exact = caster_at(2, 1, 10, 4);
    let entity = game.unit_entity(spent);
    game.get_mut::<Pools>(entity)
        .unwrap()
        .take(MANA, Num::int(91));
    let enemy = Abilities::spawn_combatant(&mut game, (5, 0, 0), Team::new(1), ());
    let far = Abilities::spawn_combatant(&mut game, (6, 0, 0), Team::new(1), ());
    let ally = Abilities::spawn_combatant(&mut game, (1, 0, 1), Team::new(0), ());
    let hidden = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
    Units::set_blocks(&mut game, hidden, &[Block::Target]);

    // An ally, an untargetable enemy, a unit beyond 5 m, and no target at all are refused; so
    // are a slot not learned, and a cost of 10 mana and 4 rage against 5 mana, 3 rage, or 9
    // mana left of 100. A refused cast spends no mana or rage, and starts no cooldown.
    let spending = |game: &Match, unit| {
        (
            Abilities::pool(game, unit),
            Abilities::pool_of(game, unit, RAGE),
            Abilities::time(game, unit).ready_at,
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
        Abilities::cast(&mut game, unit, target);
        assert_eq!(Stats::health(&game, enemy), 500, "{unit:?} at {target:?}");
        assert_eq!(spending(&game, unit), before, "{unit:?} at {target:?}");
    }
    let healths = [far, ally, hidden].map(|unit| Stats::health(&game, unit));
    assert_eq!(healths, [500, 500, 500]);
    assert_eq!(Abilities::pool(&game, caster), 100);

    // At exactly 5 m, the enemy takes 50, and the cost comes off each pool: 100 − 10 mana and
    // 20 − 4 rage. The cooldown, 1001 ms, is 30.03 ticks, rounded up to 31: the cast in tick 8 is
    // ready again in tick 39.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
    assert_eq!(Stats::health(&game, enemy), 450);
    assert_eq!(
        (
            Abilities::pool(&game, caster),
            Abilities::pool_of(&game, caster, RAGE)
        ),
        (90, 16)
    );
    assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(39));
    // A cost of exactly what is left is paid: 10 mana and 4 rage of 10 and 4, in tick 9.
    Abilities::cast(&mut game, exact, ActionTarget::Unit(enemy));
    assert_eq!(Stats::health(&game, enemy), 400);
    assert_eq!(spending(&game, exact), (0, 0, Tick::new(40)));

    // The range counts from the edge of each body: once the unit 6 m off has a body of 1 m, it
    // is within 5 m, and takes 50 in tick 39.
    game.insert_parts(far, Body::new(Num::ONE).unwrap());
    game.run_until(39);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(far));
    assert_eq!(Stats::health(&game, far), 450);
}

#[test]
fn a_cost_in_a_pool_and_a_player_resource_is_checked_and_paid_together() {
    let mut game = abilities_match();
    let gold = ResourceId::nth(0);
    let mut cost = cost("mana", int(10));
    cost.insert(DeclaredName::new("gold").unwrap(), Ranked::One(int(30)));
    let data = ActionData { cost, ..strike() };
    let strike = Abilities::load(&mut game, "strike", &data, STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let ownerless = Abilities::spawn_combatant(
        &mut game,
        (0, 0, 1),
        Team::new(0),
        ActionSlots::new([(strike, SlotKind::new(0), Rank::new(1))]).timed(),
    );
    Abilities::give_pools(&mut game, ownerless, 100, 20);
    let enemy = Abilities::spawn_combatant(&mut game, (5, 0, 0), Team::new(1), ());
    let player = PlayerSlot::new(0);
    game.player_resources_mut(player)
        .unwrap()
        .add(gold, 40)
        .unwrap();
    let held = |game: &Match, unit| {
        let amounts = game.player_resources(player).unwrap();
        (Abilities::pool(game, unit), amounts.amount(gold))
    };
    // A unit no player owns pays no player resource, so it may not cast.
    Abilities::cast(&mut game, ownerless, ActionTarget::Unit(enemy));
    assert_eq!(Stats::health(&game, enemy), 500);
    assert_eq!(Abilities::pool(&game, ownerless), 100);
    // Player 0's caster pays both in tick 1, 100 − 10 mana and 40 − 30 gold, as the strike
    // lands.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
    assert_eq!(Stats::health(&game, enemy), 450);
    assert_eq!(held(&game, caster), (90, 10));
    // Ready again in tick 1 + 31 = 32, the cooldown's 1001 ms in ticks rounded up, it may not
    // cast with 10 gold of 30: nothing is spent, and the cooldown does not start again.
    game.run_until(32);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
    assert_eq!(Stats::health(&game, enemy), 450);
    assert_eq!(held(&game, caster), (90, 10));
    assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(32));
}

#[test]
fn a_cast_its_casters_tags_stop_is_kept_and_an_interrupted_one_spends_nothing() {
    let mut game = abilities_match();
    // Strike with a windup of 100 ms, 3 ticks.
    let data = ActionData {
        kind: ActionKind::Cast,
        windup_ms: Some(Ranked::One(int(100))),
        ..strike()
    };
    let strike = Abilities::load(&mut game, "strike", &data, STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy = Abilities::spawn_combatant(&mut game, (5, 0, 0), Team::new(1), ());
    let target = ActionTarget::Unit(enemy);
    let aim = SlotAim { slot: 0, target };
    let ordered = Some(InProgress::Order {
        aim,
        phase: OrderPhase::Ordered,
    });
    // Each start from where the caster stands, the origin.
    let start = ActionStart {
        origin: Position::from((0, 0, 0)),
        charge: None,
    };
    let started = |tick| {
        Some(InProgress::Order {
            aim,
            phase: OrderPhase::Started(Started {
                resolves_at: Tick::new(tick),
                start,
            }),
        })
    };
    // A stun in tick 7's Move stage, after the casts start in Act and before they resolve in Hit.
    let stunned = &[Block::Move, Block::Attack, Block::Cast, Block::Use];
    Units::block_at(&mut game, caster, 7, SimSet::Move, stunned);

    // Silenced in tick 0 and 1: the order is kept, and not started.
    Units::set_blocks(&mut game, caster, &[Block::Cast]);
    Abilities::cast(&mut game, caster, target);
    game.run_until(2);
    assert_eq!(Abilities::casting(&game, caster), ordered);
    // Free in tick 2: it starts, to resolve in tick 5. Silenced again in tick 3: its windup
    // is interrupted, back to the order, and spends nothing.
    Units::set_blocks(&mut game, caster, &[]);
    game.run_until(3);
    assert_eq!(Abilities::casting(&game, caster), started(5));
    Units::set_blocks(&mut game, caster, &[Block::Cast]);
    game.run_until(4);
    assert_eq!(Abilities::casting(&game, caster), ordered);
    // Free in tick 4: it starts again, to resolve in tick 7, when the stun in Move holds it
    // back from resolving: back to the order once more.
    Units::set_blocks(&mut game, caster, &[]);
    game.run_until(8);
    assert_eq!(Abilities::casting(&game, caster), ordered);
    assert_eq!(
        (Stats::health(&game, enemy), Abilities::pool(&game, caster)),
        (500, 100)
    );
    assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(0));
    // Free in tick 8: it starts and resolves in tick 11, for 50 and 10 of the pool; ready again
    // 31 ticks later, in tick 42.
    Units::set_blocks(&mut game, caster, &[]);
    game.run_until(12);
    assert_eq!(Abilities::casting(&game, caster), None);
    assert_eq!(
        (Stats::health(&game, enemy), Abilities::pool(&game, caster)),
        (450, 90)
    );
    assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(42));
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
    let thrown = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.pos, 10, "enemies") { ctx.damage(unit, 50, "magic"); } throw "out" }"#;
    let cases = [
        (spin, FailureKind::CallLimit),
        (
            wrong_kind,
            FailureKind::Api(ApiError::Unknown(NameKind::DamageKind)),
        ),
        (
            undeclared,
            FailureKind::Api(ApiError::Unknown(NameKind::Param)),
        ),
        (overflow, FailureKind::Raised(Some(NumError::Overflow))),
        (thrown, FailureKind::Raised(None)),
    ];
    for (script, expected) in cases {
        // Two matches alike, but only one gets the cast order.
        let mut hashes = Vec::new();
        let mut spent = Vec::new();
        for ordered in [true, false] {
            let mut game = abilities_match();
            let action = Abilities::load(&mut game, "ability", &data, script);
            let caster = Abilities::caster(&mut game, action, 1);
            let enemy = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
            if ordered {
                Abilities::cast(&mut game, caster, ActionTarget::None);
                let failed = FailedCall {
                    id: Some(caster),
                    hook: Hook::OnResolve,
                    kind: expected,
                };
                assert_eq!(Abilities::failed_calls(&game), [failed], "{script}");
                let mut budgets = game.resource_mut::<ScriptBudgets>();
                spent.push(
                    ScriptLimits::ROOMY.player
                        - budgets.get_mut(Pool::Player(PlayerSlot::new(0))).left(),
                );
            } else {
                game.step();
            }
            assert_eq!(Stats::health(&game, enemy), 500);
            assert_eq!(Abilities::pool(&game, caster), 100);
            assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(0));
            hashes.push(game.state_hash());
        }
        assert_eq!(hashes[0], hashes[1], "{script}");
        if script == spin {
            // The call ran exactly its limit of operations.
            assert_eq!(spent, [ScriptLimits::ROOMY.per_call.get()]);
        }
    }
}

#[test]
fn a_point_aim_beyond_the_range_clamps_in_to_it_when_the_action_says_so() {
    // 10 true damage to each enemy within half a meter of the point the cast resolves at.
    let script = r#"
fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, target, num(1) / 2, "enemies") {
        ctx.damage(unit, 10, "true");
    }
}
"#;
    let leap = |clamp| ActionData {
        script: Some(PackagePath::parse("leap.rhai").unwrap()),
        range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
            Num::int(4),
        )))),
        clamp_to_range: clamp,
        ..ActionData::cast(Targeting::Point)
    };
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let clamped = Abilities::load(&mut game, "leap", &leap(true), script);
    let plain = Abilities::load(&mut game, "hop", &leap(false), script);
    let point = |x, z| ActionTarget::Point(Position::from((x, 0, z)));
    let enemy =
        |game: &mut Match, x, z| Abilities::spawn_combatant(game, (x, 0, z), Team::new(1), ());
    // From the origin, 4 m of range: an aim at 10 m along x lands at 4 m; one at 2 m stays.
    for (aim, at) in [(point(10, 0), (4, 0)), (point(2, 0), (2, 0))] {
        let caster = Abilities::caster(&mut game, clamped, 1);
        let struck = enemy(&mut game, at.0, at.1);
        Abilities::cast(&mut game, caster, aim);
        assert_eq!(Stats::health(&game, struck), 490, "{at:?}");
    }
    // An aim at (9, 12), 15 m off, lands on its line at 4 m: (2.4, 3.2), rounded once each, and
    // a bit shorter where that rounding ends past the range; the enemy there is struck.
    let caster = Abilities::caster(&mut game, clamped, 1);
    let struck = Abilities::spawn_combatant(
        &mut game,
        Position::try_from(Vec3::from((Num::int(12) / 5, 0, Num::int(16) / 5))).unwrap(),
        Team::new(1),
        (),
    );
    Abilities::cast(&mut game, caster, point(9, 12));
    assert_eq!(Stats::health(&game, struck), 490);
    // An action that does not clamp starts no cast at a point beyond its range.
    let caster = Abilities::caster(&mut game, plain, 1);
    let missed = enemy(&mut game, 4, 0);
    Abilities::cast(&mut game, caster, point(10, 0));
    assert_eq!(Stats::health(&game, missed), 500);
    // A caster with a box of 4 by 2 m at the origin reaches 4 m from the box's edge: an aim at
    // (10, 0) lands at 6 m, 4 m past its edge at x = 2, exactly, and strikes an enemy at 5.6 m.
    // One at (9, 12) leaves the box through its top, then nears its corner (2, 1): it lands where
    // the corner is 4 m off, t = 2 + √15 ≈ 5.873 m along, at about (3.524, 4.698), and strikes an
    // enemy 0.3 m short of it, at (3.34, 4.46). Measured from the box's bound, 2.24 m round its
    // center, each would land 4 + 2.24 m along, 0.24 m and 0.36 m further, past both enemies'
    // half meter.
    let boxed = |game: &mut Match| {
        let caster = Abilities::caster(game, clamped, 1);
        let body = BodyBox::new([Num::int(4), Num::int(2)], Num::ZERO).unwrap();
        game.insert_parts(caster, Body::of_box(body));
        caster
    };
    let near = |x: &str, z: &str| {
        Position::try_from(Vec3::from((
            x.parse::<Num>().unwrap(),
            0,
            z.parse::<Num>().unwrap(),
        )))
        .unwrap()
    };
    let caster = boxed(&mut game);
    let struck = Abilities::spawn_combatant(&mut game, near("5.6", "0"), Team::new(1), ());
    Abilities::cast(&mut game, caster, point(10, 0));
    assert_eq!(Stats::health(&game, struck), 490);
    let caster = boxed(&mut game);
    let struck = Abilities::spawn_combatant(&mut game, near("3.34", "4.46"), Team::new(1), ());
    Abilities::cast(&mut game, caster, point(9, 12));
    assert_eq!(Stats::health(&game, struck), 490);
    assert_eq!(Abilities::failed_calls(&game), []);
}

/// Hop, which deals 10 to each enemy within half a meter of its point, and Jab, which deals 10 to
/// its target, in a match of units that walk and take orders; each reaches 4 m, and resolves as
/// it starts.
#[derive(Debug, Clone, Copy)]
struct Reaching {
    hop: ActionId,
    jab: ActionId,
}

impl Reaching {
    /// The capabilities a reaching match declares.
    const CAPABILITIES: [Capability; 5] = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
    ];

    /// Loads Hop and Jab into `game`, a match of `CAPABILITIES`.
    fn place(game: &mut Match) -> Reaching {
        let hop = ActionData {
            script: Some(PackagePath::parse("hop.rhai").unwrap()),
            range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
                Num::int(4),
            )))),
            ..ActionData::cast(Targeting::Point)
        };
        let jab = ActionData {
            script: Some(PackagePath::parse("jab.rhai").unwrap()),
            targeting: Targeting::Unit(FilterData::parse("enemies").unwrap()),
            ..hop.clone()
        };
        let hop_script = r#"
    fn on_resolve(ctx, caster, target) {
    for unit in ctx.find(caster, target, num(1) / 2, "enemies") {
        ctx.damage(unit, 10, "true");
    }
    }
    "#;
        let jab_script =
            r#"fn on_resolve(ctx, caster, target) { ctx.damage(target, 10, "true"); }"#;
        let hop = Abilities::load(game, "hop", &hop, hop_script);
        let jab = Abilities::load(game, "jab", &jab, jab_script);
        Reaching { hop, jab }
    }
}

/// A unit that walks `meters` a tick.
fn walker(meters: Num) -> impl Bundle {
    Navigation::walker(MoveStep::new(meters).unwrap())
}

#[test]
fn a_cast_beyond_its_range_walks_in_range_first_and_starts_there() {
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &Reaching::CAPABILITIES);
    let reaching = Reaching::place(&mut game);
    // A hop at (10, 0) from the origin, a meter a tick: the Act stage of tick t finds the caster
    // at t, before that tick's step, and 10 − t m within 4 m first in tick 6, where it stops and
    // hops.
    let hopper = Abilities::walking_caster(&mut game, reaching.hop);
    let still = Abilities::spawn_combatant(&mut game, (10, 0, 0), Team::new(1), ());
    Abilities::cast(
        &mut game,
        hopper,
        ActionTarget::Point(Position::from((10, 0, 0))),
    );
    assert!(game.part::<ActionSlots>(hopper).approaching());
    game.run_until(6);
    assert_eq!(
        (
            game.part::<Position>(hopper).get().x,
            Stats::health(&game, still)
        ),
        (Num::int(6), 500)
    );
    game.step();
    assert_eq!(
        (
            game.part::<Position>(hopper).get().x,
            Stats::health(&game, still)
        ),
        (Num::int(6), 490)
    );
    game.run_until(20);
    assert_eq!(game.part::<Position>(hopper).get().x, Num::int(6));

    // A jab at an enemy at 12 m that walks away half a meter a tick: the Act stage of tick t
    // finds it at 12 + t/2 and the caster at t, 12 − t/2 m apart, within 4 m first in tick 16.
    let jabber = Abilities::walking_caster(&mut game, reaching.jab);
    let start = game.resource::<SimTick>().start().get();
    let fleeing =
        Abilities::spawn_combatant(&mut game, (12, 0, 0), Team::new(1), walker(halves(1)));
    let away = Destination::to(Some(Position::from((100, 0, 0))));
    game.insert_parts(fleeing, away);
    Abilities::cast(&mut game, jabber, ActionTarget::Unit(fleeing));
    game.run_until(start + 16);
    assert_eq!(Stats::health(&game, fleeing), 500);
    game.step();
    assert_eq!(
        (
            game.part::<Position>(jabber).get().x,
            Stats::health(&game, fleeing)
        ),
        (Num::int(16), 490)
    );
    assert_eq!(Abilities::failed_calls(&game), []);
}

#[test]
fn a_new_order_a_block_or_a_failed_check_ends_a_walk_in_range() {
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &Reaching::CAPABILITIES);
    let reaching = Reaching::place(&mut game);
    let mover = Abilities::walking_caster(&mut game, reaching.jab);
    let far = Abilities::spawn_combatant(&mut game, (0, 0, 30), Team::new(1), ());
    let send = |game: &mut Match, action| {
        let payload = Order::payload(&[Order::one(mover, action)]);
        game.step_with(&[(PlayerSlot::new(0), &payload)]);
    };
    let jab = Action::Slot {
        slot: 0,
        target: ActionTarget::Unit(far),
    };
    let approaching = |game: &Match| game.part::<ActionSlots>(mover).approaching();
    // A move order ends the walk to jab, and the jab: the unit walks where it was told.
    send(&mut game, jab);
    assert!(approaching(&game));
    let back = Position::from((0, 0, -5));
    send(
        &mut game,
        Action::Move {
            x: back.get().x,
            z: back.get().z,
        },
    );
    assert_eq!(game.part::<ActionSlots>(mover).in_progress(), None);
    assert_eq!(game.part::<Destination>(mover).get(), Some(back));
    // A silence keeps the jab as an order, and the unit stands until it ends; then it walks on.
    send(&mut game, jab);
    assert!(approaching(&game));
    Units::set_blocks(&mut game, mover, &[Block::Cast]);
    game.step();
    let ordered = InProgress::Order {
        aim: SlotAim {
            slot: 0,
            target: ActionTarget::Unit(far),
        },
        phase: OrderPhase::Ordered,
    };
    assert_eq!(game.part::<ActionSlots>(mover).in_progress(), Some(ordered));
    assert_eq!(game.part::<Destination>(mover).get(), None);
    Units::set_blocks(&mut game, mover, &[]);
    game.step();
    assert!(approaching(&game));
    // A target that dies drops the jab, and stops the walk where it is.
    let entity = game.unit_entity(far);
    game.entity_mut(entity).insert(Dead);
    game.step();
    assert_eq!(game.part::<ActionSlots>(mover).in_progress(), None);
    assert_eq!(game.part::<Destination>(mover).get(), None);
    assert_eq!(Abilities::failed_calls(&game), []);
}

#[test]
fn charges_are_spent_one_a_cast_and_come_back_one_at_a_time() {
    // Step: 3 charges, one back each 1000 ms, 30 ticks at 30 a second, and a lockout of 100 ms,
    // 3 ticks, between two casts; 10 mana a cast. Refill gives Step a charge and takes 500 ms,
    // 15 ticks, off the time to its next; Dud has no charges to give.
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let plain = ActionData {
        script: Some(PackagePath::parse("cast.rhai").unwrap()),
        ..ActionData::cast(Targeting::None)
    };
    let step = ActionData {
        charges: Some(ChargesData {
            max: Ranked::One(int(3)),
            recharge_ms: Ranked::One(int(1000)),
        }),
        cooldown_ms: Some(Ranked::One(int(100))),
        cost: cost("mana", int(10)),
        ..plain.clone()
    };
    let idle = "fn on_resolve(ctx, caster, target) {}";
    let step = Abilities::load(&mut game, "step", &step, idle);
    let refill = r#"fn on_resolve(ctx, caster, target) {
        ctx.add_charge(caster, "step");
        ctx.reduce_cooldown(caster, "step", 500);
    }"#;
    let refill = Abilities::load(&mut game, "refill", &plain, refill);
    let dud = r#"fn on_resolve(ctx, caster, target) { ctx.add_charge(caster, "dud"); }"#;
    let dud = Abilities::load(&mut game, "dud", &plain, dud);
    let caster = Abilities::spawn_combatant(
        &mut game,
        (0, 0, 0),
        Team::new(0),
        (
            Owner::new(PlayerSlot::new(0)),
            ActionSlots::new([
                (step, SlotKind::new(0), Rank::new(1)),
                (refill, SlotKind::new(0), Rank::new(1)),
                (dud, SlotKind::new(0), Rank::new(1)),
            ])
            .timed(),
        ),
    );
    Abilities::give_pools(&mut game, caster, 100, 20);
    let charges = |game: &Match| {
        let charges = Abilities::time(game, caster).charges.unwrap();
        (charges.count, charges.next.get())
    };
    let held = |game: &Match| (charges(game), Abilities::pool(game, caster));
    let order = |game: &mut Match, slot| {
        game.part_mut::<ActionSlots>(caster)
            .order(slot, ActionTarget::None);
        game.step();
    };
    // Tick 0: full, then one spent: the next comes back 30 ticks later. Tick 1 is in the lockout.
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(held(&game), ((2, 30), 90));
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(held(&game), ((2, 30), 90));
    // Ticks 3 and 6 spend the other two; tick 9 finds none, and spends nothing.
    game.run_until(3);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    game.run_until(6);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(charges(&game), (0, 30));
    game.run_until(9);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(held(&game), ((0, 30), 70));
    // Tick 10: Refill gives one back and brings the next to tick 15.
    order(&mut game, 1);
    assert_eq!(charges(&game), (1, 15));
    // One a recharge: in tick 15, then 45, and none past the most from tick 75.
    game.run_until(15);
    game.step();
    assert_eq!(charges(&game), (2, 45));
    game.run_until(45);
    game.step();
    assert_eq!(charges(&game), (3, 75));
    game.run_until(200);
    assert_eq!(charges(&game), (3, 75));
    assert_eq!(Abilities::failed_calls(&game), []);
    // Refill at full adds none; Dud names an ability with no charges, and fails.
    order(&mut game, 1);
    assert_eq!(charges(&game).0, 3);
    order(&mut game, 2);
    let failed = Abilities::failed_calls(&game);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].kind, FailureKind::Api(ApiError::NoCharges));
    // A unit that holds Step unlearned has no charges for Refill to add to.
    let learner = Abilities::spawn_combatant(
        &mut game,
        (5, 0, 0),
        Team::new(0),
        (
            Owner::new(PlayerSlot::new(0)),
            ActionSlots::new([
                (step, SlotKind::new(0), Rank::new(0)),
                (refill, SlotKind::new(0), Rank::new(1)),
            ])
            .timed(),
        ),
    );
    Abilities::give_pools(&mut game, learner, 100, 20);
    game.part_mut::<ActionSlots>(learner)
        .order(1, ActionTarget::None);
    game.step();
    let unlearned = game.part::<ActionSlots>(learner).slot(0).unwrap();
    let time = game.part::<SlotTimes>(learner).slot(0).unwrap();
    assert_eq!((unlearned.rank, time.charges), (None, None));
    assert_eq!(Abilities::failed_calls(&game), []);
}

#[test]
fn a_client_predicts_a_resolved_casts_slot_as_the_server_resolves_it() {
    // One caster casts, a tick each from tick 0: Step, of 3 charges back in 1000 ms, 30 ticks,
    // and a cooldown of 100 ms, 3 ticks; Aura, a toggle that pays each second; then Drain, a
    // channel of 300 ms that ticks each 100 ms. The server resolves each cast, and a client,
    // with no script, predicts it: after each tick their slots are alike.
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let cast = || ActionData::cast(Targeting::None);
    let step = ActionData {
        charges: Some(ChargesData {
            max: Ranked::One(int(3)),
            recharge_ms: Ranked::One(int(1000)),
        }),
        cooldown_ms: Some(Ranked::One(int(100))),
        ..cast()
    };
    let aura = ActionData {
        toggle: Some(Toggle::CostPerSecond(BTreeMap::new())),
        ..cast()
    };
    let drain = ActionData {
        channel: Some(ChannelData {
            duration_ms: Ranked::One(int(300)),
            tick_ms: Ranked::One(int(100)),
        }),
        ..cast()
    };
    let caster = |game: &mut Match| {
        let actions = [("step", &step), ("aura", &aura), ("drain", &drain)];
        let slots = actions.map(|(name, data)| {
            let action = Actions::load(game, 0, name, data, None, 1).unwrap();
            (action, SlotKind::new(0), Rank::new(1))
        });
        Units::spawn(
            game,
            (0, 0, 0),
            Team::new(0),
            ActionSlots::new(slots).timed(),
        )
    };
    let mut server = Match::server(&declared, ScriptBudgets::new(ScriptLimits::ROOMY, 1));
    let mut client = Match::client(&declared);
    let on_server = caster(&mut server);
    let on_client = caster(&mut client);
    for slot in 0..3 {
        for (game, id) in [(&mut server, on_server), (&mut client, on_client)] {
            game.part_mut::<ActionSlots>(id)
                .order(slot, ActionTarget::None);
            game.step();
        }
        let slots = server.part::<ActionSlots>(on_server);
        assert_eq!(slots, client.part::<ActionSlots>(on_client), "slot {slot}");
        let times = server.part::<SlotTimes>(on_server);
        assert_eq!(times, client.part::<SlotTimes>(on_client), "slot {slot}");
    }
    // Step spent a charge in tick 0, its next back in tick 30, and is ready from tick 3; Aura,
    // cast in tick 1, pays from tick 31; Drain, cast in tick 2, channels from tick 3, ticking in
    // tick 6, to tick 12.
    let slots = server.part::<ActionSlots>(on_server);
    let times = server.part::<SlotTimes>(on_server);
    let step = times.slot(0).unwrap();
    let charges = step.charges.unwrap();
    assert_eq!(
        (charges.count, charges.next, step.ready_at),
        (2, Tick::new(30), Tick::new(3))
    );
    assert_eq!(times.slot(1).unwrap().toggle, Some(Tick::new(31)));
    let Some(InProgress::Channel { next, ends, .. }) = slots.in_progress() else {
        panic!("Drain channels");
    };
    assert_eq!((next, ends), (Tick::new(6), Tick::new(12)));
}

#[test]
fn a_channel_ticks_from_the_tick_after_its_cast_and_an_order_a_stun_a_death_or_its_slot_emptied_cuts_it()
 {
    // Drain: a channel of 300 ms, 9 ticks at 30 a second, that ticks each 100 ms, 3 ticks, and
    // holds Ward, a shield of 100. Each tick strikes the enemies within 5 m for 10; a cut one
    // strikes its target for 7.
    let script = r#"
fn on_channel_tick(ctx, caster) {
    for unit in ctx.find(caster, caster.pos, 5, "enemies") {
        ctx.damage(unit, 10, "true");
    }
}
fn on_interrupt(ctx, caster, target) {
    ctx.damage(target, 7, "true");
}
"#;
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let ward = ModifierData {
        shield: Some(int(100)),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, 0, "ward", &ward, None);
    let drain = ActionData {
        script: Some(PackagePath::parse("drain.rhai").unwrap()),
        range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
            Num::int(5),
        )))),
        channel: Some(ChannelData {
            duration_ms: Ranked::One(int(300)),
            tick_ms: Ranked::One(int(100)),
        }),
        hold: Some(DeclaredName::new("ward").unwrap()),
        ..ActionData::cast(Targeting::Unit(FilterData::parse("enemies").unwrap()))
    };
    let drain = Abilities::load(&mut game, "drain", &drain, script);
    let ward = Stats::modifier(&game, 0, "ward").unwrap();
    // Cast in tick 0 by a caster at x, an enemy beside it; then `cut` in tick 5, if any.
    let run = |game: &mut Match, x: i16, cut: &dyn Fn(&mut Match, StableId)| {
        let caster = Abilities::caster(game, drain, 1);
        game.insert_parts(caster, (Modifiers::default(), ModifierClocks::default()));
        *game.part_mut::<Position>(caster) = Position::from((x, 0, 0));
        let target = Abilities::spawn_combatant(game, (x + 1, 0, 0), Team::new(1), ());
        Abilities::cast(game, caster, ActionTarget::Unit(target));
        let mut seen = Vec::new();
        for tick in 1..=12 {
            if tick == 5 {
                cut(game, caster);
            }
            game.step();
            let entity = game.unit_entity(caster);
            let modifiers = game.get::<Modifiers>(entity).unwrap();
            let held = modifiers.get(ward, Some(caster)).is_some();
            seen.push((Stats::health(game, target), held));
        }
        assert_eq!(Abilities::failed_calls(game), []);
        seen
    };
    // Whole: it runs from tick 1, ticks in ticks 4, 7 and 10, its last at its end, and holds
    // Ward from its first tick's Resolve to its end.
    let whole = run(&mut game, 0, &|_, _| {});
    let health = |seen: &[(i64, bool)]| seen.iter().map(|(health, _)| *health).collect::<Vec<_>>();
    assert_eq!(
        health(&whole),
        [500, 500, 500, 490, 490, 490, 480, 480, 480, 470, 470, 470]
    );
    let held: Vec<bool> = whole.iter().map(|(_, held)| *held).collect();
    assert_eq!(
        held,
        [
            true, true, true, true, true, true, true, true, true, false, false, false
        ]
    );
    // A new order in tick 5 cuts it, after its tick 4: its `on_interrupt` strikes for 7 then, and
    // it ticks no more; so does a stun in tick 5.
    let cut = [500, 500, 500, 490, 483, 483, 483, 483, 483, 483, 483, 483];
    let ordered = run(&mut game, 20, &|game, caster| {
        let mut slots = game.part_mut::<ActionSlots>(caster);
        slots.order(0, ActionTarget::None);
    });
    assert_eq!(health(&ordered), cut);
    let stunned = run(&mut game, 40, &|game, caster| {
        Units::set_blocks(game, caster, &[Block::Cast]);
    });
    assert_eq!(health(&stunned), cut);
    // Death in tick 5's Resolve cuts it too; its `on_interrupt` runs in the next tick's Hit.
    let killed = run(&mut game, 60, &|game, caster| {
        let damage = Num::int(1000);
        Combat::queue_damage(game, None, caster, damage, "true");
    });
    let late = [500, 500, 500, 490, 490, 483, 483, 483, 483, 483, 483, 483];
    assert_eq!(health(&killed), late);
    // A slot emptied in tick 5, as an item sold empties its own, cuts it as an order does: its
    // `on_interrupt` is the action it channeled, which the slot no longer holds.
    let emptied = run(&mut game, 80, &|game, caster| {
        let entity = game.unit_entity(caster);
        let mut unit = game.entity_mut(entity);
        let (mut slots, mut times) = unit
            .get_components_mut::<(&mut ActionSlots, &mut SlotTimes)>()
            .unwrap();
        slots.fill(&mut times, 0, None);
    });
    assert_eq!(health(&emptied), cut);
}

#[test]
fn a_charged_cast_resolves_at_its_release_or_full_with_its_share_and_its_origin() {
    // Draw: charges up to 3000 ms, 90 ticks, for 10 mana at its release. It deals its target its
    // share times 90, and ten times how far the caster stands from where it started.
    let script = r#"
fn on_resolve(ctx, caster, target) {
    ctx.damage(target, ctx.charge * 90 + ctx.origin.distance_to(caster.pos) * 10, "true");
}
"#;
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let draw = ActionData {
        script: Some(PackagePath::parse("draw.rhai").unwrap()),
        range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
            Num::int(5),
        )))),
        charge: Some(ChargeUpData {
            max_ms: Ranked::One(int(3000)),
        }),
        cost: cost("mana", int(10)),
        ..ActionData::cast(Targeting::Unit(FilterData::parse("enemies").unwrap()))
    };
    let plain = ActionData {
        charge: None,
        ..draw.clone()
    };
    let draw = Abilities::load(&mut game, "draw", &draw, script);
    let plain = Abilities::load(&mut game, "plain", &plain, script);
    let other = Abilities::load(&mut game, "other", &lash_out(), LASH_OUT);
    let caster = |game: &mut Match, x: i16| {
        let pos = Position::from((x, 0, 0));
        let slots = ActionSlots::new([
            (draw, SlotKind::new(0), Rank::new(1)),
            (other, SlotKind::new(0), Rank::new(1)),
        ])
        .timed();
        let caster = Abilities::spawn_combatant(
            game,
            pos,
            Team::new(0),
            (Owner::new(PlayerSlot::new(0)), slots),
        );
        Abilities::give_pools(game, caster, 100, 20);
        let target = Abilities::spawn_combatant(game, (x + 3, 0, 0), Team::new(1), ());
        (caster, target)
    };
    let order = |game: &mut Match, id: StableId, slot: u8, target: ActionTarget| {
        game.part_mut::<ActionSlots>(id).order(slot, target);
    };
    let run_to = |game: &mut Match, tick: u64| game.run_until(tick);
    let share = |held: i64| Num::int(held).checked_div(Num::int(90)).unwrap();

    // Ordered in tick 0, it charges; in tick 10 the caster moves 2 m, which keeps the charge; its
    // order again in tick 30 releases it: 30 of 90 ticks, a third, as 24 fraction bits round it.
    let (released, target) = caster(&mut game, 0);
    order(&mut game, released, 0, ActionTarget::Unit(target));
    run_to(&mut game, 10);
    *game.part_mut::<Position>(released) = Position::from((-2, 0, 0));
    run_to(&mut game, 30);
    order(&mut game, released, 0, ActionTarget::Unit(target));
    game.step();
    let dealt = share(30) * Num::int(90) + Num::int(20);
    assert_eq!(game.part::<Life>(target).current(), Num::int(500) - dealt);
    assert_eq!(Abilities::pool(&game, released), 90);
    // Never released, it resolves full in tick 90 of its start: all 90.
    let start = game.now().get();
    let (full, target) = caster(&mut game, 20);
    order(&mut game, full, 0, ActionTarget::Unit(target));
    run_to(&mut game, start + 91);
    assert_eq!(game.part::<Life>(target).current(), Num::int(410));
    // Another action's order cancels it, and so does a stun: the draw spends nothing. Lash Out,
    // ordered instead, hits the target 3 m away for its 75 and costs its 35.
    for (cut, life, mana) in [(0_i16, 425, 65), (1, 500, 100)] {
        let (cancelled, target) = caster(&mut game, 40 + 20 * cut);
        order(&mut game, cancelled, 0, ActionTarget::Unit(target));
        game.step();
        if cut == 0 {
            order(&mut game, cancelled, 1, ActionTarget::None);
        } else {
            Units::set_blocks(&mut game, cancelled, &[Block::Cast]);
        }
        game.step();
        let now = game.now().get();
        run_to(&mut game, now + 100);
        assert_eq!(Stats::health(&game, target), life, "cut {cut}");
        assert_eq!(Abilities::pool(&game, cancelled), mana, "cut {cut}");
    }
    assert_eq!(Abilities::failed_calls(&game), []);
    // The same script in an action that does not charge fails its call.
    let pos = Position::from((100, 0, 0));
    let slots = ActionSlots::new([(plain, SlotKind::new(0), Rank::new(1))]).timed();
    let caster = Abilities::spawn_combatant(
        &mut game,
        pos,
        Team::new(0),
        (Owner::new(PlayerSlot::new(0)), slots),
    );
    Abilities::give_pools(&mut game, caster, 100, 20);
    let target = Abilities::spawn_combatant(&mut game, (103, 0, 0), Team::new(1), ());
    Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
    let failed = Abilities::failed_calls(&game);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].kind, FailureKind::Api(ApiError::NotCharged));
}

#[test]
fn a_cast_reads_its_range_at_its_rank() {
    // 10 damage a meter of range; 1 for a global range, which reads as `()`.
    let script = r#"
fn on_resolve(ctx, caster, target) {
    if ctx.range == () {
        ctx.damage(target, 1, "true");
    } else {
        ctx.damage(target, ctx.range * 10, "true");
    }
}
"#;
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let near = Abilities::load(&mut game, "strike", &strike(), script);
    let global = ActionData {
        range: Some(Ranked::One(RangeField::Range(ActionRange::Global))),
        ..strike()
    };
    let far = Abilities::load(&mut game, "far", &global, script);
    for (action, damage) in [(near, 50), (far, 1)] {
        let caster = Abilities::caster(&mut game, action, 1);
        let target = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
        Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
        assert_eq!(Stats::health(&game, target), 500 - damage);
    }
    assert_eq!(Abilities::failed_calls(&game), []);
}

#[test]
fn each_caster_draws_from_its_own_sequence() {
    // Each caster's Lash Out strikes the enemy beside it for two draws: tens, then ones.
    let script = r#"
fn on_resolve(ctx, caster, target) {
    let digits = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    for unit in ctx.find(caster, caster.pos, ctx.p.radius, "enemies") {
        ctx.damage(unit, ctx.pick(digits) * 10 + ctx.pick(digits), "true");
    }
}
"#;
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let action = Abilities::load(&mut game, "lash_out", &lash_out(), script);
    let first = Abilities::caster(&mut game, action, 1);
    let second = Abilities::spawn_combatant(
        &mut game,
        (20, 0, 0),
        Team::new(0),
        (
            Owner::new(PlayerSlot::new(0)),
            ActionSlots::new([(action, SlotKind::new(0), Rank::new(1))]).timed(),
        ),
    );
    Abilities::give_pools(&mut game, second, 100, 20);
    let targets = [Num::int(1), Num::int(21)].map(|x| {
        Abilities::spawn_combatant(
            &mut game,
            Position::try_from(Vec3::from((x, 0, 0))).unwrap(),
            Team::new(1),
            (),
        )
    });
    Abilities::casts(
        &mut game,
        &[(first, ActionTarget::None), (second, ActionTarget::None)],
    );
    assert_eq!(Abilities::failed_calls(&game), []);
    // The sequence of `script.draw` for each caster in this tick, opened as the world opens it.
    let opener = game.resource::<SimRng>().opener();
    let sequence = |caster: StableId| opener.open(RngStream::new("script.draw"), caster.get());
    let damage = |mut rng: Rng| {
        let tens = i64::try_from(rng.pick(9)).unwrap() + 1;
        let ones = i64::try_from(rng.pick(9)).unwrap() + 1;
        tens * 10 + ones
    };
    let expected = [first, second].map(|caster| 500 - damage(sequence(caster)));
    assert_eq!(targets.map(|target| Stats::health(&game, target)), expected);
    // The two sequences are not one.
    let words = |caster| {
        let mut rng = sequence(caster);
        [rng.next_u64(), rng.next_u64()]
    };
    assert_ne!(words(first), words(second));
}

#[test]
fn a_cast_draws_from_its_casters_player_pool() {
    // Each player's pool holds one whole call: player 0's spinning cast spends all of its pool,
    // and player 1's strike in the same tick still resolves from its own, which it spends as it
    // does alone.
    let mut left = Vec::new();
    for spins in [true, false] {
        let limits = ScriptLimits {
            player: ScriptLimits::ROOMY.per_call.get(),
            ..ScriptLimits::ROOMY
        };
        let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
        let mut game = abilities_match_with(limits, &declared);
        let data = ActionData {
            kind: ActionKind::Cast,
            params: BTreeMap::new(),
            ..lash_out()
        };
        let spin = Abilities::load(
            &mut game,
            "spin",
            &data,
            "fn on_resolve(ctx, caster, target) { loop {} }",
        );
        let strike = Abilities::load(&mut game, "strike", &strike(), STRIKE);
        let spinner = Abilities::caster(&mut game, spin, 1);
        let striker = Abilities::spawn_combatant(
            &mut game,
            (0, 0, 1),
            Team::new(0),
            (
                Owner::new(PlayerSlot::new(1)),
                ActionSlots::new([(strike, SlotKind::new(0), Rank::new(1))]).timed(),
            ),
        );
        Abilities::give_pools(&mut game, striker, 100, 20);
        let enemy = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
        let casts = [
            (spinner, ActionTarget::None),
            (striker, ActionTarget::Unit(enemy)),
        ];
        Abilities::casts(&mut game, if spins { &casts } else { &casts[1..] });
        let limited = FailedCall {
            id: Some(spinner),
            hook: Hook::OnResolve,
            kind: FailureKind::CallLimit,
        };
        let expected: Vec<_> = spins.then_some(limited).into_iter().collect();
        assert_eq!(Abilities::failed_calls(&game), expected);
        // The strike's 50 true damage: 500 → 450, and its cost of 10: 100 → 90.
        assert_eq!(Stats::health(&game, enemy), 450);
        assert_eq!(Abilities::pool(&game, striker), 90);
        let mut budgets = game.resource_mut::<ScriptBudgets>();
        let spent = if spins {
            0
        } else {
            ScriptLimits::ROOMY.per_call.get()
        };
        assert_eq!(
            budgets.get_mut(Pool::Player(PlayerSlot::new(0))).left(),
            spent
        );
        left.push(budgets.get_mut(Pool::Player(PlayerSlot::new(1))).left());
    }
    assert_eq!(left[0], left[1]);
    assert!(left[0] < ScriptLimits::ROOMY.per_call.get());
}

#[test]
fn an_ability_loads_only_when_its_data_holds() {
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    // Each load under its own name, as a package names each action once.
    let load = |game: &mut Match, name: &str, data: &ActionData, source: &str| {
        let script = Units::compile_hooked(game, source).unwrap();
        Actions::load(game, 0, name, data, Some(script), 5)
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
    assert_eq!(
        [lash_out().check_ranks(5), uneven.check_ranks(5)],
        [true, false]
    );
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
        assert_eq!(
            data.fields_at(Rank::FIRST, pool).err(),
            Some(field),
            "{field:?}"
        );
    }
    // What only a match's rate decides: i64::MAX ms counts in no tick.
    let forever = load(&mut game, "forever", &forever, LASH_OUT);
    assert_eq!(forever, Err(TimeTooLarge));
    assert_eq!(
        load(&mut game, "lash_out", &lash_out(), LASH_OUT),
        Ok(ActionId::new(0))
    );
    // A direction loads, as every targeting does; no cast can aim one yet.
    assert_eq!(
        load(&mut game, "aimed", &aimed, LASH_OUT),
        Ok(ActionId::new(1))
    );

    // A script may serve only the ability's modifiers: a cast of rank 1 in tick 0 then runs no
    // script, and spends 35 of 100 and its 10 000 ms, 300 ticks at 30 a second.
    let modifiers_only = "fn on_damage_taken(ctx, m, d) { }";
    let passive = load(&mut game, "passive", &lash_out(), modifiers_only).unwrap();
    let caster = Abilities::caster(&mut game, passive, 1);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(Abilities::failed_calls(&game), []);
    assert_eq!(Abilities::pool(&game, caster), 65);
    assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(300));
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
    let mut game = abilities_match();
    let strike = Units::compile_hooked(&mut game, STRIKE).unwrap();
    let id = Actions::load(&mut game, 0, "strike", &data, Some(strike), 3).unwrap();
    let book = game.resource::<ActionBook>();
    let ranks: Vec<_> = book
        .get(id)
        .unwrap()
        .ranks
        .iter()
        .map(|values| {
            (
                values.cooldown.get(),
                values.cost.get(MANA).to_int().expect("a whole cost"),
            )
        })
        .collect();
    assert_eq!(ranks, [(30, 5), (60, 7), (90, 9)]);
    // Three ranks of an array of 3 is the rank count; five is not.
    assert_eq!([data.check_ranks(3), data.check_ranks(5)], [true, false]);
}

#[test]
fn a_unit_target_is_one_its_filter_selects_tag_and_all() {
    let mut game = abilities_match();
    let mut load_type = |name: &str| {
        let data = UnitTypeCore::tagged(&[name]);
        Units::load_type(&mut game, TypeScope::Mode, name, &data)
    };
    let (hero, creep) = (load_type("avatar"), load_type("creep"));
    let mut heroes_only = strike();
    heroes_only.targeting = Targeting::Unit(FilterData::parse("enemies:avatar").unwrap());
    let strike = Abilities::load(&mut game, "strike", &heroes_only, STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy_creep = Abilities::spawn_combatant_of(&mut game, creep, (3, 0, 0), Team::new(1), ());
    let enemy_hero = Abilities::spawn_combatant_of(&mut game, hero, (4, 0, 0), Team::new(1), ());
    // The creep is an enemy, but no hero: the cast goes nowhere. The hero takes 50.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy_creep));
    assert_eq!(Stats::health(&game, enemy_creep), 500);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy_hero));
    assert_eq!(Stats::health(&game, enemy_hero), 450);
}

#[test]
fn a_passive_is_held_while_its_ability_has_a_rank_and_is_ready() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    // A guard whose shield is Lash Out's damage at its rank: 75, then 100.
    let guard = ModifierData {
        shield: Some(param("damage")),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, 0, "guard", &guard, None);
    let mut data = lash_out();
    data.passive_modifier = Some(DeclaredName::new("guard").unwrap());
    data.passive_while_ready = true;
    let action = Abilities::load(&mut game, "lash_out", &data, LASH_OUT);
    let caster = Abilities::caster(&mut game, action, 0);
    let entity = game.unit_entity(caster);
    game.insert_parts(caster, (Modifiers::default(), ModifierClocks::default()));
    let id = Stats::modifier(&game, 0, "guard").unwrap();
    let shield = |game: &Match| {
        let modifiers = game.get::<Modifiers>(entity).unwrap();
        let held = modifiers.get(id, Some(caster))?;
        assert!(held.lifetime.held_by(Hold::Passive) && held.lifetime.until().is_none());
        let clocks = game.get::<ModifierClocks>(entity).unwrap();
        clocks.shield_of(modifiers, id, Some(caster))
    };
    let learn = |game: &mut Match| {
        let mut slots = game.get_mut::<ActionSlots>(entity).unwrap();
        slots.learn(0);
        game.step();
    };
    // Unlearned, none; at rank 1, the shield of 75; at rank 2, applied again, 100.
    game.step();
    assert_eq!(shield(&game), None);
    learn(&mut game);
    assert_eq!(shield(&game), Some(Num::int(75)));
    learn(&mut game);
    assert_eq!(shield(&game), Some(Num::int(100)));
    // A cast puts it on cooldown for 9000 ms at rank 2, 270 ticks: gone from that tick, back in
    // the 270th after it.
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(shield(&game), None);
    for _ in 0..269 {
        game.step();
    }
    assert_eq!(shield(&game), None);
    game.step();
    assert_eq!(shield(&game), Some(Num::int(100)));

    // A weapon's passive holds as well, in a match with no abilities: a ward of 40 from its
    // first tick.
    let mut game = abilities_match_with(
        ScriptLimits::ROOMY,
        &[Capability::Stats, Capability::Combat],
    );
    Abilities::load_stats(&mut game);
    let ward = ModifierData {
        shield: Some(int(40)),
        ..guard
    };
    Stats::load_modifier(&mut game, 0, "ward", &ward, None);
    let mut claws = lash_out();
    claws.kind = ActionKind::Attack;
    claws.script = None;
    claws.cooldown_ms = None;
    claws.cost = BTreeMap::new();
    claws.params = BTreeMap::new();
    claws.targeting = Targeting::Unit(FilterData::parse("enemies").unwrap());
    claws.range = Some(Ranked::One(RangeField::Range(ActionRange::Meters(
        Num::int(2),
    ))));
    claws.rate = Some(Stat::named("armor").unwrap());
    claws.damage = Some(Stat::named("attack_damage").unwrap());
    claws.damage_kind = Some(DeclaredName::new("physical").unwrap());
    claws.passive_modifier = Some(DeclaredName::new("ward").unwrap());
    let claws = Actions::load(&mut game, 0, "claws", &claws, None, 1).unwrap();
    let slots = ActionSlots::new([(claws, SlotKind::new(0), Rank::new(1))]).timed();
    let beast = Abilities::spawn_combatant(&mut game, (0, 0, 0), Team::new(0), slots);
    let entity = game.unit_entity(beast);
    game.insert_parts(beast, (Modifiers::default(), ModifierClocks::default()));
    game.step();
    let ward = Stats::modifier(&game, 0, "ward").unwrap();
    let modifiers = game.get::<Modifiers>(entity).unwrap();
    let clocks = game.get::<ModifierClocks>(entity).unwrap();
    assert!(modifiers.get(ward, Some(beast)).is_some());
    assert_eq!(
        clocks.shield_of(modifiers, ward, Some(beast)),
        Some(Num::int(40))
    );
}

#[test]
fn a_cast_applies_a_modifier_from_its_caster_with_its_abilitys_params() {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    let mark = ModifierData {
        duration_ms: Some(int(1000)),
        shield: Some(param("damage")),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, 0, "mark", &mark, None);
    // Package 1 names its own `mark`, of no shield.
    let other = ModifierData {
        shield: None,
        ..mark.clone()
    };
    Stats::load_modifier(&mut game, 1, "mark", &other, None);
    let marker = r#"
fn on_resolve(ctx, caster, target) {
    let m = ctx.add_modifier(caster, "mark");
    if m.stacks != 1 { throw "a new modifier's handle has one stack"; }
}
"#;
    let action = Abilities::load(&mut game, "lash_out", &lash_out(), marker);
    let caster = Abilities::caster(&mut game, action, 3);
    let entity = game.unit_entity(caster);
    game.insert_parts(caster, (Modifiers::default(), ModifierClocks::default()));
    let t = game.resource::<SimTick>().start();
    Abilities::cast(&mut game, caster, ActionTarget::None);
    // From the caster, by Lash Out at rank 3: a shield of its damage there, 125; 1000 ms at 30
    // ticks a second, 30 ticks, so it ends as tick t + 31 starts.
    let id = Stats::modifier(&game, 0, "mark").unwrap();
    let modifiers = game.get::<Modifiers>(entity).unwrap();
    let held = modifiers.get(id, Some(caster)).unwrap();
    assert_eq!(
        (held.action, held.rank),
        (Some(action), Rank::new(3).unwrap())
    );
    let clocks = game.get::<ModifierClocks>(entity).unwrap();
    assert_eq!(
        (
            clocks.shield_of(modifiers, id, Some(caster)),
            held.lifetime.until()
        ),
        (Some(Num::int(125)), Some(Tick::new(t.get() + 31)))
    );

    // A cast of package 1's ability means package 1's names: the caster carries package 0's
    // `mark`, not its own, and the cast adds its own beside it.
    let own = r#"
fn on_resolve(ctx, caster, target) {
    if caster.has_modifier("mark") { throw "the caster carries no mark of package 1"; }
    ctx.add_modifier(caster, "mark");
}
"#;
    let script = Units::compile_hooked(&mut game, own).unwrap();
    let theirs = Actions::load(&mut game, 1, "lash_out", &lash_out(), Some(script), 5);
    let slots = ActionSlots::new([(theirs.unwrap(), SlotKind::new(0), Rank::new(1))]).timed();
    game.entity_mut(entity).insert(slots);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(Abilities::failed_calls(&game), []);
    let book = game.resource::<ModifierBook>();
    let ids = [0, 1].map(|package| book.names().named((package, "mark")).unwrap());
    let modifiers = game.get::<Modifiers>(entity).unwrap();
    let clocks = game.get::<ModifierClocks>(entity).unwrap();
    let carried = ids.map(|id| modifiers.get(id, Some(caster)).is_some());
    let shields = ids.map(|id| clocks.shield_of(modifiers, id, Some(caster)));
    assert_eq!(
        (carried, shields),
        ([true, true], [Some(Num::int(125)), None])
    );
}

/// A match in which a strike stuns its target for 100 ms through the mode's `stunned` tag: each
/// tick's state hash, and whether the target's tags block its moving.
fn stun_run() -> Vec<(StateHash, bool)> {
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    let stun = ModifierData {
        script: None,
        tags: vec![DeclaredName::new("stunned").unwrap()],
        ..scripted(None, &[])
    };
    Stats::load_modifier(&mut game, 0, "stun", &stun, None);
    // Every type loads before the tag book takes their tags.
    let target_type = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "target",
        &UnitTypeCore::default(),
    );
    let stunned = TagData {
        blocks: vec![Block::Move, Block::Attack, Block::Cast, Block::Use],
        ..TagData::default()
    };
    let effects = BTreeMap::from([(DeclaredName::new("stunned").unwrap(), stunned)]);
    Units::seal_types_with(&mut game, &effects);
    let script = r#"fn on_resolve(ctx, caster, target) { ctx.add_modifier(target, "stun", 100); }"#;
    let strike = Abilities::load(&mut game, "strike", &strike(), script);
    let caster = Abilities::caster(&mut game, strike, 1);
    let parts = (
        Level::default(),
        UnitStats::default(),
        (Modifiers::default(), ModifierClocks::default()),
    );
    let enemy =
        Abilities::spawn_combatant_of(&mut game, target_type, (5, 0, 0), Team::new(1), parts);
    let entity = game.unit_entity(enemy);
    let mut seen = Vec::new();
    for tick in 0..8 {
        if tick == 2 {
            Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
        } else {
            game.step();
        }
        let stunned = UnitTags::properties_of(game.get(entity)).blocks(Block::Move);
        seen.push((game.state_hash(), stunned));
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
fn a_purge_ends_the_applications_of_the_modifiers_that_grant_its_tag() {
    // Two casters' lists purge `stunned` in tick 0, each from its own target. The first carries
    // a stun from each caster and a slow from the second: both stuns end, whatever their source,
    // and the slow, which grants `slowed`, stays, so its tags no longer stop it moving. The
    // second carries a stun from the first caster that its passive holds and an application
    // renewed, and a stun from the second caster: the application ends, the hold keeps the first
    // stun, as its passive would apply it again, and the second ends.
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    for (name, tag) in [("slow", "slowed"), ("stun", "stunned")] {
        let data = ModifierData {
            tags: vec![DeclaredName::new(tag).unwrap()],
            ..ModifierData::default()
        };
        Stats::load_modifier(&mut game, 0, name, &data, None);
    }
    // Every type loads before the tag book takes their tags.
    let target = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "target",
        &UnitTypeCore::default(),
    );
    let stunned = TagData {
        blocks: vec![Block::Move],
        ..TagData::default()
    };
    let effects = BTreeMap::from([(DeclaredName::new("stunned").unwrap(), stunned)]);
    Units::seal_types_with(&mut game, &effects);
    let purge = DoesData::Purge(PurgeFields {
        tag: DeclaredName::new("stunned").unwrap(),
    });
    let data = ActionData {
        script: None,
        on_resolve: vec![effect(purge, EffectTo::Reached)],
        ..strike()
    };
    let action = Actions::load(&mut game, 0, "cleanse", &data, None, 1).unwrap();
    Actions::load_effect_lists(&mut game, action, 0, &data);
    let [first, second] = [(); 2].map(|()| Abilities::caster(&mut game, action, 1));
    let parts = || {
        (
            Level::default(),
            UnitStats::default(),
            (Modifiers::default(), ModifierClocks::default()),
        )
    };
    let freed = Abilities::spawn_combatant_of(&mut game, target, (1, 0, 0), Team::new(1), parts());
    let held = Abilities::spawn_combatant_of(&mut game, target, (-1, 0, 0), Team::new(1), parts());
    for source in [first, second] {
        Abilities::give_from(&mut game, freed, source, "stun");
    }
    Abilities::give_from(&mut game, freed, second, "slow");
    let [stun, slow] = ["stun", "slow"].map(|name| Stats::modifier(&game, 0, name).unwrap());
    Stats::give_modifier(&mut game, held, stun, Some((first, None, 1)), true);
    Abilities::give_from(&mut game, held, first, "stun");
    Abilities::give_from(&mut game, held, second, "stun");
    game.step();
    let stopped = |game: &Match, id| {
        let entity = game.unit_entity(id);
        UnitTags::properties_of(game.get(entity)).blocks(Block::Move)
    };
    assert_eq!([stopped(&game, freed), stopped(&game, held)], [true, true]);
    Abilities::casts(
        &mut game,
        &[
            (first, ActionTarget::Unit(freed)),
            (second, ActionTarget::Unit(held)),
        ],
    );
    assert_eq!(Abilities::failed_calls(&game), []);
    assert_eq!(
        Stats::carried_modifiers(&game, freed),
        [(slow, Some(second))]
    );
    assert_eq!(Stats::carried_modifiers(&game, held), [(stun, Some(first))]);
    assert_eq!([stopped(&game, freed), stopped(&game, held)], [false, true]);
}

#[test]
fn a_cast_heals_and_restores_and_a_negative_amount_fails_it() {
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let mender = "
fn on_resolve(ctx, caster, target) {
    ctx.heal(caster, 30);
    ctx.restore(caster, \"mana\", num(20));
}
";
    let action = Abilities::load(&mut game, "lash_out", &lash_out(), mender);
    let caster = Abilities::caster(&mut game, action, 1);
    game.part_mut::<Life>(caster).take(Num::int(460));
    game.part_mut::<Pools>(caster).take(MANA, Num::int(50));
    // From 40 health and 50 mana: 30 healed and 20 restored as the effects apply, then the
    // cost of 35: 70 and 35.
    Abilities::cast(&mut game, caster, ActionTarget::None);
    assert_eq!(
        (Stats::health(&game, caster), Abilities::pool(&game, caster)),
        (70, 35)
    );

    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let negative = "fn on_resolve(ctx, caster, target) { ctx.restore(caster, \"mana\", 5); ctx.heal(caster, -1); }";
    let action = Abilities::load(&mut game, "lash_out", &lash_out(), negative);
    let caster = Abilities::caster(&mut game, action, 1);
    Abilities::cast(&mut game, caster, ActionTarget::None);
    let failed = FailedCall {
        id: Some(caster),
        hook: Hook::OnResolve,
        kind: FailureKind::Api(ApiError::NegativeHeal),
    };
    assert_eq!(Abilities::failed_calls(&game), [failed]);
    assert_eq!(Abilities::pool(&game, caster), 100);
}

/// A match of stats, combat, abilities, progression and `more`, with the stats the scaling
/// params name and the track `valor`, a level at 50, which is not the unit's level.
fn with_valor(more: &[Capability]) -> (Match, TrackId) {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Abilities,
        Capability::Progression,
    ];
    let declared: Vec<Capability> = declared.into_iter().chain(more.iter().copied()).collect();
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    let valor = TrackData {
        levels: Thresholds::new([Num::int(50)]).unwrap(),
        level: false,
    };
    let tracks = BTreeMap::from([(DeclaredName::new("valor").unwrap(), valor)]);
    Progression::load_tracks(&mut game, &tracks);
    (game, TrackId::new(0))
}

/// `effecting` to `to`.
const fn effect(effecting: DoesData, to: EffectTo) -> EffectData {
    EffectData {
        does: effecting,
        to,
    }
}

/// 10 experience on `valor`.
fn valor_xp() -> DoesData {
    DoesData::Xp(XpFields {
        track: DeclaredName::new("valor").unwrap(),
        amount: int(10),
    })
}

/// The `damage` param as true damage.
fn true_damage() -> DoesData {
    DoesData::Damage(DamageFields {
        amount: param("damage"),
        kind: DeclaredName::new("true").unwrap(),
    })
}

#[test]
fn an_xp_effect_to_a_unit_without_the_track_fails_its_cast_as_add_xp_does() {
    // Strike's list deals its 50 damage to the unit it aims at, then gives it 10 experience on
    // `valor`: a unit with the track takes both, and the cast pays its 10 mana; a unit without
    // the track fails the cast, which changes nothing, its damage, cost and cooldown included.
    let (mut game, valor) = with_valor(&[]);
    let data = ActionData {
        script: None,
        on_resolve: vec![
            effect(true_damage(), EffectTo::Reached),
            effect(valor_xp(), EffectTo::Reached),
        ],
        ..strike()
    };
    let action = Actions::load(&mut game, 0, "strike", &data, None, 1).unwrap();
    Actions::load_effect_lists(&mut game, action, 0, &data);
    let tracked = Abilities::spawn_combatant(
        &mut game,
        (1, 0, 0),
        Team::new(1),
        Experience::new(TrackSet::of([valor]), None),
    );
    let untracked = Abilities::spawn_combatant(&mut game, (-1, 0, 0), Team::new(1), ());
    let [first, second] = [(); 2].map(|()| Abilities::caster(&mut game, action, 1));
    Abilities::cast(&mut game, first, ActionTarget::Unit(tracked));
    assert_eq!(Abilities::failed_calls(&game), []);
    let xp = game.part::<Experience>(tracked).get(valor).unwrap().xp;
    assert_eq!(
        (
            Stats::health(&game, tracked),
            xp,
            Abilities::pool(&game, first)
        ),
        (450, Num::int(10), 90)
    );
    Abilities::cast(&mut game, second, ActionTarget::Unit(untracked));
    let failed = FailedCall {
        id: Some(second),
        hook: Hook::OnResolve,
        kind: FailureKind::Api(ApiError::NoTrack),
    };
    assert_eq!(Abilities::failed_calls(&game), [failed]);
    assert_eq!(
        (
            Stats::health(&game, untracked),
            Abilities::pool(&game, second)
        ),
        (500, 100)
    );
    assert_eq!(Abilities::time(&game, second).ready_at, Tick::ZERO);
}

#[test]
fn an_xp_effect_to_a_source_that_despawned_fails_its_hit() {
    // A bolt at 0.5 m a tick whose hit deals its 50 damage to the unit it reaches and gives its
    // caster 10 experience on `valor`. The caster despawns as the bolt flies: the hit fails, and
    // the enemy 4 m out takes no damage.
    let (mut game, valor) = with_valor(&[Capability::Projectiles]);
    let bolt = Units::load_type(&mut game, TypeScope::Mode, "bolt", &UnitTypeCore::default());
    let flight = ProjectileData {
        width: halves(1),
        range: Some(Num::int(6)),
        ..ProjectileData::flying(Num::int(15))
    };
    Projectiles::load_type(&mut game, bolt, &flight);
    let data = ActionData {
        script: None,
        targeting: Targeting::Direction,
        range: None,
        delivery: Some(DeliveryData::Projectile {
            unit_type: DeclaredName::new("bolt").unwrap(),
            count: NonZeroU8::new(1).unwrap(),
            spread_deg: Num::ZERO,
        }),
        on_hit: vec![
            effect(true_damage(), EffectTo::Reached),
            effect(valor_xp(), EffectTo::Source),
        ],
        ..strike()
    };
    let action = Actions::load(&mut game, 0, "bolt", &data, None, 1).unwrap();
    Actions::load_effect_lists(&mut game, action, 0, &data);
    let caster = Abilities::caster(&mut game, action, 1);
    game.insert_parts(caster, Experience::new(TrackSet::of([valor]), None));
    let enemy = Abilities::spawn_combatant(&mut game, (4, 0, 0), Team::new(1), ());
    Abilities::cast(
        &mut game,
        caster,
        ActionTarget::Point(Position::from((10, 0, 0))),
    );
    let entity = game.unit_entity(caster);
    game.despawn(entity);
    let mut failed = Vec::new();
    while game.now() < Tick::new(16) {
        game.step();
        failed.extend(Abilities::failed_calls(&game));
    }
    let hit = FailedCall {
        id: Some(caster),
        hook: Hook::OnHit,
        kind: FailureKind::Api(ApiError::NoTrack),
    };
    assert_eq!(failed, [hit]);
    assert_eq!(Stats::health(&game, enemy), 500);
}

/// A modifier of no duration that runs `script`, with an interval of `interval_ms` and the
/// params `params`.
fn scripted(interval_ms: Option<i64>, params: &[(&str, i64)]) -> ModifierData {
    ModifierData {
        script: Some(PackagePath::parse("scripts/hooks.rhai").unwrap()),
        interval_ms: interval_ms.map(int),
        params: params
            .iter()
            .map(|&(name, value)| {
                (
                    DeclaredName::new(name).unwrap(),
                    Param::Ranked(Ranked::One(Scalar::Int(value))),
                )
            })
            .collect(),
        ..ModifierData::default()
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
    let mut game = abilities_match_with_modifiers(
        log,
        &[
            ("log", scripted(None, &[])),
            ("pulse", scripted(Some(100), &[])),
        ],
    );
    game.insert_resource(AssistWindow(Ticks::new(10)));
    let victim = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
    let origin = Position::from((0, 0, 0));
    let attacker = Abilities::attacker(&mut game, origin, Team::new(0), striker(500), victim);
    let assister = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(0), ());
    let bystander = Abilities::spawn_combatant(&mut game, (9, 0, 0), Team::new(1), ());
    for unit in [attacker, assister, victim] {
        Abilities::give(&mut game, unit, "log");
    }
    Abilities::give(&mut game, bystander, "pulse");
    // The attacker strikes for 500 as its windup of no ticks ends, in tick 0; the assister
    // struck the victim in tick 0 too.
    let target = game.unit_entity(victim);
    game.resource_scope(|world, index: Mut<'_, EntityIndex>| {
        let mut attackers = world.get_mut::<RecentAttackers>(target).unwrap();
        attackers.record(assister, Tick::new(0), &index);
    });
    game.step();
    assert_eq!(Stats::health(&game, victim), 0);
    assert_eq!(
        Abilities::calls(&game),
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
        game.step();
        let expected = if tick % 3 == 0 {
            vec![(bystander, Hook::OnInterval)]
        } else {
            Vec::new()
        };
        assert_eq!(Abilities::calls(&game), expected, "tick {tick}");
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
    let mut game = abilities_match_with_modifiers(
        hooks,
        &[
            ("double", scripted(None, &[])),
            ("echo", scripted(None, &[("echo", 1)])),
        ],
    );
    let victim = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
    let origin = Position::from((0, 0, 0));
    let attacker = Abilities::attacker(&mut game, origin, Team::new(0), striker(30), victim);
    let echoer = Abilities::spawn_combatant(&mut game, (9, 0, 0), Team::new(1), ());
    Abilities::give(&mut game, attacker, "double");
    Abilities::give(&mut game, echoer, "echo");
    game.resource_mut::<PassQueue>().push_damage(Damage {
        source: None,
        target: echoer,
        amount: Num::int(10),
        kind: DamageKind::new(2),
        cause: DamageCause::Effect,
        action: None,
        depth: 0,
        hit: None,
    });
    game.step();
    // The attack's 30, then the extra attack's 30, which adds none: 500 − 60. The echoer's 10,
    // then an echo of 1 from each hook at depths 1 to 15; the one at 16 fails: 500 − 10 − 15.
    assert_eq!(Stats::health(&game, victim), 440);
    assert_eq!(Stats::health(&game, echoer), 475);
    let failed = FailedCall {
        id: Some(echoer),
        hook: Hook::OnDamageTaken,
        kind: FailureKind::Api(ApiError::ChainTooDeep),
    };
    assert_eq!(Abilities::failed_calls(&game), [failed]);
}

#[test]
fn a_hook_heal_is_dealt_before_the_rest_of_the_pass() {
    // A unit that heals itself by its modifier's `mend`, twice, each time true damage reaches it.
    let hooks = r#"
fn on_damage_taken(ctx, m, d) {
    if d.kind == "true" {
        ctx.heal(m.carrier, ctx.p.mend);
        ctx.heal(m.carrier, ctx.p.mend);
    }
}
"#;
    let mut game =
        abilities_match_with_modifiers(hooks, &[("mend", scripted(None, &[("mend", 30)]))]);
    let carrier = Abilities::spawn_combatant(&mut game, (0, 0, 0), Team::new(1), ());
    Abilities::give(&mut game, carrier, "mend");
    for (amount, kind) in [(450, 2), (100, 0)] {
        game.resource_mut::<PassQueue>().push_damage(Damage {
            source: None,
            target: carrier,
            amount: Num::int(amount),
            kind: DamageKind::new(kind),
            cause: DamageCause::Effect,
            action: None,
            depth: 0,
            hit: None,
        });
    }
    game.step();
    // 500 − 450, then the hook's two heals of 30 before the 100 queued after the 450:
    // 50 + 30 + 30 − 100. Dealt at the end of the queue, they would come after the 100, which
    // kills.
    assert_eq!(Stats::health(&game, carrier), 10);
    assert_eq!(Abilities::failed_calls(&game), []);
}

#[test]
fn a_learn_order_spends_a_point_on_the_next_rank_its_level_allows() {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
    ];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    Abilities::load_stats(&mut game);
    // A guard whose shield is Lash Out's damage at its rank, which its passive holds: 75 at rank
    // 1, then 25 more a rank. Its 5 ranks need levels 1, 1, 3, 3 and 5.
    let guard = ModifierData {
        shield: Some(param("damage")),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, 0, "guard", &guard, None);
    let mut data = lash_out();
    data.passive_modifier = Some(DeclaredName::new("guard").unwrap());
    let lash_out = Abilities::load(&mut game, "lash_out", &data, LASH_OUT);
    let levels = [1, 1, 3, 3, 5].map(|level| Level::new(level).unwrap());
    let ranks = SlotRanks::new(NonZeroU8::new(5).unwrap(), Some(levels.to_vec())).unwrap();
    game.insert_resource(SlotKinds::from(vec![SlotKindData {
        name: DeclaredName::new("basic").unwrap(),
        ranks: Some(ranks),
    }]));
    // Player 0's caster, at level 1 with 2 points, its spawn's and one more.
    let caster = Abilities::caster(&mut game, lash_out, 0);
    let track = TrackId::new(0);
    let mut points = Points::at_spawn(TrackSet::of([track]), Some(track)).unwrap();
    points.gain(1);
    let step = MoveStep::new(Num::ONE).unwrap();
    let parts = (
        Level::default(),
        points,
        (Modifiers::default(), ModifierClocks::default()),
    );
    game.insert_parts(caster, (parts, Navigation::walker(step)));
    let entity = game.unit_entity(caster);
    let id = Stats::modifier(&game, 0, "guard").unwrap();
    let send = |game: &mut Match, slot: u32, actions: &[Action]| {
        let orders: Vec<_> = actions
            .iter()
            .map(|&action| Order::one(caster, action))
            .collect();
        let payload = Order::payload(&orders);
        game.step_with(&[(PlayerSlot::new(slot), &payload)]);
    };
    // Its rank, its points, and the shield its passive holds.
    let progress = |game: &Match| {
        let modifiers = game.get::<Modifiers>(entity).unwrap();
        let clocks = game.get::<ModifierClocks>(entity).unwrap();
        let shield = clocks
            .shield_of(modifiers, id, Some(caster))
            .map(|shield| shield.to_int().unwrap());
        let points = game.get::<Points>(entity).unwrap().get();
        (
            Rank::count(Abilities::slot(game, caster).rank),
            points,
            shield,
        )
    };
    let learn = Action::Learn { slot: 0 };

    // Player 1 does not control the caster: nothing. Player 0 orders a walk, which a learn in the
    // next tick does not stop: rank 1 for a point, its shield in the same tick.
    send(&mut game, 1, &[learn]);
    assert_eq!(progress(&game), (0, 2, None));
    let walk = Action::Move {
        x: Num::int(9),
        z: Num::ZERO,
    };
    send(&mut game, 0, &[walk]);
    send(&mut game, 0, &[learn]);
    assert_eq!(progress(&game), (1, 1, Some(75)));
    let destination = game.get::<Destination>(entity).unwrap().get();
    assert_eq!(destination, Some(Position::from((9, 0, 0))));
    // In tick 3, a cast ordered before two learns: the first learns rank 2 with the last point,
    // the second finds none, and the cast starts at rank 2, after the learns of its tick, with
    // rank 2's cooldown of 9000 ms, 270 ticks.
    let cast = Action::Slot {
        slot: 0,
        target: ActionTarget::None,
    };
    send(&mut game, 0, &[cast, learn, learn]);
    assert_eq!(progress(&game), (2, 0, Some(100)));
    assert_eq!(Abilities::time(&game, caster).ready_at, Tick::new(3 + 270));
    // Rank 3 needs level 3: refused at level 1 with 4 points, learned at level 3 though dead.
    game.part_mut::<Points>(caster).gain(4);
    send(&mut game, 0, &[learn]);
    assert_eq!(progress(&game), (2, 4, Some(100)));
    *game.part_mut::<Level>(caster) = Level::new(3).unwrap();
    game.insert_parts(caster, Dead);
    send(&mut game, 0, &[learn]);
    let (rank, points, _) = progress(&game);
    assert_eq!((rank, points), (3, 3));
    game.entity_mut(entity).remove::<Dead>();
    // At level 5, ranks 4 and 5; a third learn finds the last rank, and spends nothing.
    *game.part_mut::<Level>(caster) = Level::new(5).unwrap();
    send(&mut game, 0, &[learn, learn, learn]);
    assert_eq!(progress(&game), (5, 1, Some(175)));
    assert_eq!(Abilities::failed_calls(&game), []);
}

/// A match of stats, combat, abilities, projectiles and areas whose `fighter` type has 40 attack
/// damage and 1 armor, which its weapons read as their damage and their rate, with a homing
/// `bolt` of 30 m a second; `mark`, a shield of the `sap` param of the action that applies it;
/// `ward`, a shield of 100; and `blast` and `ring`, areas of 2 m on enemies, of no delay and no
/// duration.
fn weapon_match() -> (Match, UnitType) {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Abilities,
        Capability::Projectiles,
        Capability::Areas,
    ];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    let fighter = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "fighter",
        &UnitTypeCore::default(),
    );
    let flat = |value| StatValue {
        base: Num::int(value),
        per_level: Num::ZERO,
    };
    let [attack_damage, armor] = ["attack_damage", "armor"].map(|name| Stat::named(name).unwrap());
    let growth = StatsData([(attack_damage, flat(40)), (armor, flat(1))].into());
    let rules: BTreeMap<_, _> = scaling_stats()
        .map(|stat| (stat, StatRule::default()))
        .into();
    let book = StatBook::new(&rules, [(fighter, &growth)], Num::int(6));
    Stats::load_book(&mut game, book);
    let bolt = Units::load_type(&mut game, TypeScope::Mode, "bolt", &UnitTypeCore::default());
    let flight = ProjectileData {
        homing: true,
        ..ProjectileData::flying(Num::int(30))
    };
    Projectiles::load_type(&mut game, bolt, &flight);
    for name in ["blast", "ring"] {
        let area = Units::load_type(&mut game, TypeScope::Mode, name, &UnitTypeCore::default());
        let data = AreaData {
            radius: Num::int(2),
            delay_ms: 0,
            duration_ms: 0,
            affects: None,
            inside: AreaInsideData::default(),
        };
        Areas::load_type(&mut game, area, 0, &data);
    }
    for (name, shield) in [("mark", param("sap")), ("ward", int(100))] {
        let data = ModifierData {
            shield: Some(shield),
            ..ModifierData::default()
        };
        Stats::load_modifier(&mut game, 0, name, &data, None);
    }
    (game, fighter)
}

/// A weapon of no windup and 1 attack a second, of the attacker's attack damage, physical: within
/// 2 m, or within 6 m firing a `bolt`. Its `on_hit` shields the target by `mark`, of its `sap`
/// param of 40, then deals its `bite` param as true damage, 10 plus half the attacker's attack
/// damage.
fn sapper(ranged: bool) -> ActionData {
    let meters = if ranged { 6 } else { 2 };
    let delivery = ranged.then(|| DeliveryData::Projectile {
        unit_type: DeclaredName::new("bolt").unwrap(),
        count: NonZeroU8::MIN,
        spread_deg: Num::ZERO,
    });
    let bite = scaling(
        Ranked::One(Num::int(10)),
        0,
        &[("attack_damage", Num::HALF)],
        &[],
    );
    ActionData {
        kind: ActionKind::Attack,
        script: None,
        range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
            Num::int(meters),
        )))),
        rate: Some(Stat::named("armor").unwrap()),
        damage: Some(Stat::named("attack_damage").unwrap()),
        damage_kind: Some(DeclaredName::new("physical").unwrap()),
        delivery,
        params: BTreeMap::from([
            (
                DeclaredName::new("sap").unwrap(),
                Param::Ranked(Ranked::One(Scalar::Int(40))),
            ),
            (DeclaredName::new("bite").unwrap(), bite),
        ]),
        on_hit: vec![
            effect(
                DoesData::Modifier(ModifierFields {
                    id: DeclaredName::new("mark").unwrap(),
                    duration_ms: None,
                }),
                EffectTo::Reached,
            ),
            effect(
                DoesData::Damage(DamageFields {
                    amount: param("bite"),
                    kind: DeclaredName::new("true").unwrap(),
                }),
                EffectTo::Reached,
            ),
        ],
        ..ActionData::cast(Targeting::Unit(FilterData::parse("enemies").unwrap()))
    }
}

#[test]
fn a_weapons_on_hit_list_follows_each_attack_that_reaches_its_target() {
    // A melee attack on a target 1 m out strikes as its windup of no ticks ends, in tick 0; a
    // bolt launched then flies 1 m a tick from tick 1 and reaches a target 4 m out in tick 4; a
    // cast whose script
    // calls `ctx.attack_hit` strikes with the caster's weapon in tick 0. Each deals 40, 500 → 460;
    // then its list shields the target by `mark`, from the attacker with the weapon's `sap` of
    // 40, at once, and queues its `bite` of 10 + 0.5 × 40 = 30 at the end of the pass, which the
    // shield absorbs: 460 left, and 10 of the shield.
    let extra = "fn on_resolve(ctx, caster, target) { ctx.attack_hit(target); }";
    for (row, ranged, by_cast, hits_in) in [
        ("melee", false, false, 0),
        ("ranged", true, false, 4),
        ("extra", false, true, 0),
    ] {
        let (mut game, fighter) = weapon_match();
        let weapon = Abilities::load_weapon(&mut game, "sapper", &sapper(ranged));
        let out = if ranged { 4 } else { 1 };
        let target = Abilities::spawn_combatant(
            &mut game,
            (out, 0, 0),
            Team::new(1),
            (Modifiers::default(), ModifierClocks::default()),
        );
        let attacker = if by_cast {
            let data = ActionData {
                range: Some(Ranked::One(RangeField::Range(ActionRange::Meters(
                    Num::int(5),
                )))),
                ..strike()
            };
            let cast = Abilities::load(&mut game, "swing", &data, extra);
            let slots = ActionSlots::new([
                (cast, SlotKind::new(0), Rank::new(1)),
                (weapon, SlotKind::new(1), Rank::new(1)),
            ])
            .timed();
            let caster = Abilities::fighter(&mut game, fighter, slots);
            Abilities::give_pools(&mut game, caster, 100, 20);
            Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
            caster
        } else {
            let slots = ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))]).timed();
            let attacker = Abilities::fighter(&mut game, fighter, slots);
            game.part_mut::<ActionSlots>(attacker)
                .set_attack_target(Some(target));
            attacker
        };
        if by_cast {
            assert_eq!(Abilities::failed_calls(&game), [], "{row}");
        }
        while game.now() < Tick::new(hits_in) {
            assert_eq!(
                (
                    Stats::health(&game, target),
                    Abilities::mark(&game, target, attacker)
                ),
                (500, None),
                "{row} before its hit"
            );
            game.step();
            assert_eq!(Abilities::failed_calls(&game), [], "{row}");
        }
        if !by_cast {
            game.step();
            assert_eq!(Abilities::failed_calls(&game), [], "{row}");
        }
        assert_eq!(
            (
                Stats::health(&game, target),
                Abilities::mark(&game, target, attacker)
            ),
            (460, Some(Num::int(10))),
            "{row}"
        );
    }
}

/// `damage` true damage of `amount`.
fn true_of(amount: Number) -> DoesData {
    DoesData::Damage(DamageFields {
        amount,
        kind: DeclaredName::new("true").unwrap(),
    })
}

/// A launch of `area` with its lists `on_hit` and `on_end`.
fn launch(area: &str, on_hit: Vec<EffectData>, on_end: Vec<EffectData>) -> DoesData {
    DoesData::Launch(LaunchFields {
        area: DeclaredName::new(area).unwrap(),
        on_hit,
        on_end,
    })
}

#[test]
fn a_launch_lands_its_area_where_it_reaches_and_runs_the_lists_it_holds_once_each() {
    // A cannon within 2 m, of no windup, whose `on_hit` launches a blast, which deals its `splash`
    // param, half the attacker's 40 attack damage, 20, to each enemy within 2 m, and whose
    // `on_end` launches a ring where the attacker stands, which deals 5 to each enemy within
    // 2 m of it.
    let (mut game, fighter) = weapon_match();
    let ring = launch(
        "ring",
        vec![effect(true_of(int(5)), EffectTo::Reached)],
        Vec::new(),
    );
    let blast = launch(
        "blast",
        vec![effect(true_of(param("splash")), EffectTo::Reached)],
        vec![effect(ring, EffectTo::Source)],
    );
    let splash = scaling(
        Ranked::One(Num::ZERO),
        0,
        &[("attack_damage", Num::HALF)],
        &[],
    );
    let cannon = ActionData {
        params: BTreeMap::from([(DeclaredName::new("splash").unwrap(), splash)]),
        on_hit: vec![effect(blast, EffectTo::Reached)],
        ..sapper(false)
    };
    let weapon = Abilities::load_weapon(&mut game, "cannon", &cannon);
    let attacker = Abilities::fighter(
        &mut game,
        fighter,
        ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))]).timed(),
    );
    let [target, near, far] = [1, 2, 4].map(|x| {
        let pos = Position::try_from(Vec3::from((
            Num::int(x) + Num::HALF * i64::from(x == 2),
            0,
            0,
        )))
        .unwrap();
        Abilities::spawn_combatant(&mut game, pos, Team::new(1), ())
    });
    game.part_mut::<ActionSlots>(attacker)
        .set_attack_target(Some(target));
    // Tick 0: the attack deals 40 in Resolve, and its list launches the blast on the target,
    // 1 m out, which lands then. Tick 1: the blast triggers in Hit, a tick after it landed; it
    // reaches the target and the unit 1.5 m past it, not the one 3 m past, and deals 20 to each;
    // it ends at its trigger, and its `on_end` lands the ring on the attacker in that Hit. Tick
    // 2: the ring reaches the target, 1 m from the attacker, alone, and deals 5. Each list runs
    // once: the next attack is 30 ticks after the first.
    let mut lives = Vec::new();
    for _ in 0..5 {
        game.step();
        lives.push([target, near, far].map(|id| Stats::health(&game, id)));
    }
    assert_eq!(Abilities::failed_calls(&game), []);
    assert_eq!(
        lives,
        [
            [460, 500, 500],
            [440, 480, 500],
            [435, 480, 500],
            [435, 480, 500],
            [435, 480, 500],
        ]
    );
    let mut areas = game.query::<&Area>();
    assert_eq!(areas.iter(&game).count(), 0);

    // The attacker despawns after tick 0, as a dead creep does: the blast it launched still
    // triggers in tick 1, and its `on_end` launches the ring where the gone attacker would stand,
    // which lands nothing, as an area whose source is gone does.
    let (mut game, fighter) = weapon_match();
    let weapon = Abilities::load_weapon(&mut game, "cannon", &cannon);
    let attacker = Abilities::fighter(
        &mut game,
        fighter,
        ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))]).timed(),
    );
    let target = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
    game.part_mut::<ActionSlots>(attacker)
        .set_attack_target(Some(target));
    game.step();
    let entity = game.unit_entity(attacker);
    game.despawn(entity);
    let mut lives = Vec::new();
    for _ in 0..3 {
        game.step();
        lives.push(Stats::health(&game, target));
    }
    // The blast's splash of 0 + 0.5 × the attack damage of a source that is gone, which has no
    // stats, is 0: the target keeps the 460 the attack left it, in every tick, and no call fails.
    assert_eq!(Abilities::failed_calls(&game), []);
    assert_eq!(lives, [460, 460, 460]);
    let mut areas = game.query::<&Area>();
    assert_eq!(areas.iter(&game).count(), 0);
}

#[test]
fn a_weapons_list_runs_one_link_down_for_each_attack_that_reaches_a_living_target() {
    // Attacks of 40 by the sapper, queued straight into the pass: on a living target at depth 0,
    // and at depth 14, the list runs at depths 1 and 15, as above; on a target whose tags block
    // damage, or at zero life, the attack does nothing and no list follows; at depth 15 the list
    // would run at 16, past the chain's limit, so it fails, recorded for the attacker, and
    // applies nothing. A target warded by 100 takes nothing of the attack, all absorbed, and the
    // list still follows: its mark of 40 holds, and the bite of 30 comes off one of the shields.
    let (mut game, fighter) = weapon_match();
    let weapon = Abilities::load_weapon(&mut game, "sapper", &sapper(false));
    let attacker = Abilities::fighter(
        &mut game,
        fighter,
        ActionSlots::new([(weapon, SlotKind::new(0), Rank::new(1))]).timed(),
    );
    let mut target = |x| {
        Abilities::spawn_combatant(
            &mut game,
            (x, 0, 0),
            Team::new(1),
            (Modifiers::default(), ModifierClocks::default()),
        )
    };
    let [living, blocked, spent, deep, deepest, warded] = [1, 2, 3, 4, 5, 6].map(&mut target);
    // A tick first, whose stats derive each new unit's tags, as for every unit that spawns: the
    // blocked target's tags, set after it, stay.
    game.step();
    Units::set_blocks(&mut game, blocked, &[Block::Damage]);
    game.part_mut::<Life>(spent).take(Num::int(500));
    Abilities::give_from(&mut game, warded, warded, "ward");
    let attack = |target, depth| Damage {
        source: Some(attacker),
        target,
        amount: Num::int(40),
        kind: DamageKind::new(0),
        cause: DamageCause::Attack {
            roll: Num::ZERO,
            rank: Rank::FIRST,
        },
        action: Some(weapon),
        depth,
        hit: None,
    };
    let mut queue = game.resource_mut::<PassQueue>();
    for (target, depth) in [
        (living, 0),
        (blocked, 0),
        (spent, 0),
        (deep, 14),
        (deepest, 15),
        (warded, 0),
    ] {
        queue.push_damage(attack(target, depth));
    }
    game.step();
    let shields =
        [living, blocked, spent, deep, deepest].map(|id| Abilities::mark(&game, id, attacker));
    let marked = Some(Num::int(10));
    assert_eq!(shields, [marked, None, None, marked, None]);
    let lives = [living, blocked, spent, deep, deepest, warded].map(|id| Stats::health(&game, id));
    assert_eq!(lives, [460, 500, 0, 460, 460, 500]);
    assert!(Abilities::mark(&game, warded, attacker).is_some());
    let failed = FailedCall {
        id: Some(attacker),
        hook: Hook::OnHit,
        kind: FailureKind::Api(ApiError::ChainTooDeep),
    };
    assert_eq!(Abilities::failed_calls(&game), [failed]);
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
        duration_ms,
        stats: stats
            .iter()
            .map(|(name, value)| (Stat::named(name).unwrap(), change(value)))
            .collect(),
        params: params
            .iter()
            .map(|(name, param)| (DeclaredName::new(name).unwrap(), param.clone()))
            .collect(),
        ..ModifierData::default()
    }
}

/// A scaling table of `base` at every rank, `per_level`, and `ratios` and `bonus` by stat name.
fn scaling(
    base: Ranked<Num>,
    per_level: i64,
    ratios: &[(&str, Num)],
    bonus: &[(&str, Num)],
) -> Param {
    let by_name = |pairs: &[(&str, Num)]| {
        pairs
            .iter()
            .map(|&(name, ratio)| (Stat::named(name).unwrap(), ratio))
            .collect()
    };
    Param::Scaling(Scaling {
        base,
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
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    // The caster's type gives attack damage 50 + 5 a level, so 60 at level 3; a modifier adds 20
    // more and 40 ability power.
    let caster_type = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "caster",
        &UnitTypeCore::default(),
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
    let book = StatBook::new(&rules, [(caster_type, &growth)], Num::int(6));
    Stats::load_book(&mut game, book);
    let boost = changing(
        &[("attack_damage", int(20)), ("ability_power", int(40))],
        &[],
        None,
    );
    Stats::load_modifier(&mut game, 0, "boost", &boost, None);
    let mark = ModifierData {
        shield: Some(param("power")),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, 0, "mark", &mark, None);
    // Power: 100 a rank, 10 a level, half the ability power and 1.5 times the bonus attack
    // damage.
    let half = Num::HALF;
    let power = scaling(
        Ranked::PerRank([100, 200, 300, 400, 500].map(Num::int).to_vec()),
        10,
        &[("ability_power", half)],
        &[("attack_damage", half * 3)],
    );
    let data = ActionData {
        kind: ActionKind::Cast,
        params: [(DeclaredName::new("power").unwrap(), power)].into(),
        ..strike()
    };
    let script = r#"
fn on_resolve(ctx, caster, target) {
    ctx.damage(target, ctx.p.power, "true");
    ctx.add_modifier(caster, "mark");
}
"#;
    let action = Abilities::load(&mut game, "ability", &data, script);
    let caster = Abilities::caster(&mut game, action, 2);
    let parts = (caster_type, Level::new(3).unwrap(), UnitStats::default());
    game.insert_parts(caster, parts);
    Abilities::give(&mut game, caster, "boost");
    let target = Abilities::spawn_combatant(
        &mut game,
        (5, 0, 0),
        Team::new(1),
        (Modifiers::default(), ModifierClocks::default()),
    );
    Abilities::cast(&mut game, caster, ActionTarget::Unit(target));

    // At rank 2 and level 3: 200 + 10 × 2 + 0.5 × 40 + 1.5 × (80 − 60) = 270, which the script
    // deals, 500 → 230, and the shield of the caster's mark holds.
    assert_eq!(Abilities::failed_calls(&game), []);
    assert_eq!(Stats::health(&game, target), 230);
    let mark = Stats::modifier(&game, 0, "mark").unwrap();
    let entity = game.unit_entity(caster);
    let modifiers = game.get::<Modifiers>(entity).unwrap();
    let clocks = game.get::<ModifierClocks>(entity).unwrap();
    let shield = clocks.shield_of(modifiers, mark, Some(caster));
    assert_eq!(shield, Some(Num::int(270)));
}

/// Veil, her type, and the places of attack damage, spell vamp and armor: Veil has attack damage 53 at level 1. Dual Path gives her spell vamp of 0.06 and
/// 0.00167 a point of bonus attack damage; Fortify gives armor of 10 times her spell vamp, so armor
/// reads spell vamp, which reads attack damage. Armor's place, before spell vamp's, makes the
/// graph's order differ from the places'. Veil holds both, and Boost of 30 attack damage is loaded.
#[derive(Debug, Clone, Copy)]
struct Veil {
    id: StableId,
    unit_type: UnitType,
    places: [usize; 3],
}

impl Veil {
    /// The capabilities Veil's match declares.
    const CAPABILITIES: [Capability; 3] =
        [Capability::Stats, Capability::Combat, Capability::Abilities];

    /// Loads Veil's type, stats and modifiers into `game`, a match of `CAPABILITIES`, and spawns
    /// her.
    fn place(game: &mut Match) -> Veil {
        let veil_type = Units::load_type(game, TypeScope::Mode, "game", &UnitTypeCore::default());
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
        let book = StatBook::new(&rules, [(veil_type, &growth)], Num::int(6))
            .with_order(graph.order().unwrap());
        let place = |stat: &Stat| book.named(stat).unwrap().index();
        let places = [&attack_damage, &spell_vamp, &armor].map(place);
        Stats::load_book(game, book);
        let vamp = scaling(
            Ranked::One(decimal("0.06")),
            0,
            &[],
            &[("attack_damage", decimal("0.00167"))],
        );
        let dual_path = changing(&[("spell_vamp", param("vamp"))], &[("vamp", vamp)], None);
        let guard = scaling(
            Ranked::One(Num::ZERO),
            0,
            &[("spell_vamp", Num::int(10))],
            &[],
        );
        let fortify = changing(&[("armor", param("guard"))], &[("guard", guard)], None);
        let boost = changing(&[("attack_damage", int(30))], &[], None);
        for (name, data) in [
            ("boost", boost),
            ("dual_path", dual_path),
            ("fortify", fortify),
        ] {
            Stats::load_modifier(game, 0, name, &data, None);
        }
        let veil = Abilities::spawn_combatant_of(
            game,
            veil_type,
            (0, 0, 0),
            Team::new(0),
            (Level::default(), UnitStats::default()),
        );
        Abilities::give(game, veil, "dual_path");
        Abilities::give(game, veil, "fortify");
        Veil {
            id: veil,
            unit_type: veil_type,
            places,
        }
    }

    /// The attack damage, spell vamp and armor of `id` in `game`.
    fn values(self, game: &Match, id: StableId) -> [Num; 3] {
        let stats = game.part::<UnitStats>(id).values();
        self.places.map(|at| stats[at])
    }
}

#[test]
fn a_live_change_follows_its_source_in_the_order_of_the_stats_it_reads() {
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &Veil::CAPABILITIES);
    let veil = Veil::place(&mut game);
    let unit = veil.id;
    game.step();
    // 0.06 is 1 006 632.96 bits, to 1 006 633; no bonus; armor 10 times that.
    assert_eq!(
        veil.values(&game, unit),
        [
            Num::int(53),
            Num::from_bits(1_006_633),
            Num::from_bits(10_066_330)
        ]
    );

    // 30 more attack damage: in the same refresh, bonus 30, and 0.00167, 28 017.95 bits to
    // 28 018, times 30 is 840 540: spell vamp 1 847 173 bits, armor ten times it.
    Abilities::give(&mut game, unit, "boost");
    game.step();
    assert_eq!(
        veil.values(&game, unit),
        [
            Num::int(83),
            Num::from_bits(1_847_173),
            Num::from_bits(18_471_730)
        ]
    );
}

#[test]
fn a_live_change_keeps_its_last_value_when_its_source_is_gone_and_none_at_no_stack() {
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &Veil::CAPABILITIES);
    let veil = Veil::place(&mut game);
    let source = veil.id;
    // Ward, of Veil's type, holds Dual Path from Veil, with Boost: 0.06 and 0.00167 of Veil's
    // bonus of 30, as above, spell vamp 1 847 173 bits.
    let ward_parts = (Level::default(), UnitStats::default());
    let ward = Abilities::spawn_combatant_of(
        &mut game,
        veil.unit_type,
        (1, 0, 0),
        Team::new(0),
        ward_parts,
    );
    Abilities::give(&mut game, source, "boost");
    Abilities::give_from(&mut game, ward, source, "dual_path");
    // A copy of the state follows the match: the value a live change last had is state, and
    // each write of it marks its change.
    let mut copy = StateCopy::of(&mut game);
    game.step();
    Release::check_copy(&mut game, &mut copy);
    let vamp = Num::from_bits(1_847_173);
    assert_eq!(veil.values(&game, ward)[1], vamp);
    // Veil's own spell vamp, and Ward's, are live changes of Veil, so both refresh as Veil does.
    let ward_entity = game.unit_entity(ward);
    let mut carriers = [(source, game.unit_entity(source)), (source, ward_entity)];
    carriers.sort_unstable();
    assert_eq!(game.resource::<LiveCarriers>().carried(), carriers);
    // Veil goes: the change keeps the value it last had, and refreshes no more, as its source
    // changes no more. Read with no source it would fall to 0.06 alone, 1 006 633 bits.
    let entity = game.unit_entity(source);
    game.despawn(entity);
    game.step();
    Release::check_copy(&mut game, &mut copy);
    assert_eq!(veil.values(&game, ward)[1], vamp);
    assert_eq!(game.resource::<LiveCarriers>().carried(), []);
    // At no stack it adds nothing, and is no live change that refreshes its unit every pass.
    let dual_path = Stats::modifier(&game, 0, "dual_path").unwrap();
    let mut modifiers = game.get_mut::<Modifiers>(ward_entity).unwrap();
    modifiers.set_stacks(dual_path, Some(source), 0, Tick::new(0));
    game.step();
    Release::check_copy(&mut game, &mut copy);
    assert_eq!(veil.values(&game, ward)[1], Num::ZERO);
    assert_eq!(game.resource::<LiveCarriers>().carried(), []);
}

#[test]
fn a_delivery_hook_reads_its_projectile_and_the_unit_its_cast_aimed_at() {
    let mut game = abilities_match_with(
        ScriptLimits::ROOMY,
        &[
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Projectiles,
        ],
    );
    let bolt = Units::load_type(&mut game, TypeScope::Mode, "bolt", &UnitTypeCore::default());
    let data = ProjectileData {
        stop_on_hit: true,
        ..ProjectileData::flying(Num::int(15))
    };
    Projectiles::load_type(&mut game, bolt, &data);
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
    let action = Abilities::load(&mut game, "shot", &shot, source);
    let caster = Abilities::caster(&mut game, action, 1);
    let target = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
    // The damage the hook deals carries the bolt's hit, which the target's `watch` reads, and
    // answers with 1 damage to its source: the hit's target, the unit the cast aimed at, after
    // 3 m flown, in the direction it flew.
    Abilities::load_stats(&mut game);
    let watch = r#"
        fn on_damage_taken(ctx, m, d) {
            if d.hit == () || d.hit.target != m.carrier || d.hit.distance != 3
                || d.hit.direction == () {
                throw "the damage carries the hit of its bolt";
            }
            ctx.damage(d.source, 1, "true");
        }
    "#;
    let script = Units::compile_hooked(&mut game, watch).unwrap();
    Stats::load_modifier(&mut game, 0, "watch", &scripted(None, &[]), Some(script));
    Abilities::give(&mut game, target, "watch");

    // The bolt launches in tick 0 and flies half a meter a tick from tick 1: it reaches the
    // target 3 m out in tick 6, hits it, and ends there, as it stops on a hit.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
    game.run_until(7);
    assert_eq!(
        (Stats::health(&game, target), Stats::health(&game, caster)),
        (450, 492)
    );
    assert_eq!(Abilities::failed_calls(&game), []);
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
            let mut game = abilities_match_with(
                ScriptLimits::ROOMY,
                &[
                    Capability::Stats,
                    Capability::Combat,
                    Capability::Abilities,
                    Capability::Projectiles,
                ],
            );
            let bolt =
                Units::load_type(&mut game, TypeScope::Mode, "bolt", &UnitTypeCore::default());
            let data = ProjectileData {
                homing,
                stop_on_hit: true,
                ..ProjectileData::flying(Num::int(15))
            };
            Projectiles::load_type(&mut game, bolt, &data);
            let shot = ActionData {
                delivery: Some(DeliveryData::Projectile {
                    unit_type: DeclaredName::new("bolt").unwrap(),
                    count: NonZeroU8::MIN,
                    spread_deg: Num::ZERO,
                }),
                ..strike()
            };
            let source = format!("fn on_resolve(ctx, caster, target) {{ {call}; }}");
            let action = Abilities::load(&mut game, "shot", &shot, &source);
            let caster = Abilities::caster(&mut game, action, 1);
            let target = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
            Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
            game.run_until(1);
            let mut projectiles = game.query::<&Projectile>();
            let launched = projectiles.iter(&game).count();
            let failed: Vec<_> = Abilities::failed_calls(&game)
                .into_iter()
                .map(|failure| failure.kind)
                .collect();
            if homing == at_unit {
                assert_eq!((launched, failed), (2, vec![]), "{homing} {call}");
            } else {
                let other = FailureKind::Api(ApiError::OtherFlight);
                assert_eq!((launched, failed), (0, vec![other]), "{homing} {call}");
            }
        }
    }
}

#[test]
fn a_delivery_a_script_creates_spawns_with_the_id_its_call_took_and_the_state_it_wrote() {
    let enemy = Team::new(1);
    // Each cast delivers its own projectile or area, and its `on_resolve` one more, whose
    // `charge` it writes and reads back. The caster and the target take ids 0 and 1; the call
    // takes 2 for its delivery, and the cast's own takes 3 as it spawns later in the tick. A
    // cast whose call fails after its write delivers nothing and takes no id: the next unit to
    // spawn takes 2.
    let state = |charge: i64| vec![StateValue::Int(charge), StateValue::Int(7)];
    for area in [false, true] {
        for fails in [false, true] {
            let delivery = if area {
                Capability::Areas
            } else {
                Capability::Projectiles
            };
            let declared = [
                Capability::Stats,
                Capability::Combat,
                Capability::Abilities,
                delivery,
            ];
            let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
            let field = |default| SyncedStateDecl {
                decl: StateDecl::new(StateType::Int, Some(StateDefault::Int(default))).unwrap(),
                sync: SyncTo::None,
            };
            let data = UnitTypeCore {
                state: [("charge", field(0)), ("mark", field(7))]
                    .map(|(name, field)| (DeclaredName::new(name).unwrap(), field))
                    .into(),
                ..UnitTypeCore::default()
            };
            let bolt = Units::load_type(&mut game, TypeScope::Mode, "bolt", &data);
            let (delivery, call) = if area {
                let data = AreaData {
                    radius: Num::int(2),
                    delay_ms: 100,
                    duration_ms: 0,
                    affects: None,
                    inside: AreaInsideData::default(),
                };
                Areas::load_type(&mut game, bolt, 0, &data);
                let unit_type = DeclaredName::new("bolt").unwrap();
                (DeliveryData::Area { unit_type }, "ctx.area(target.pos)")
            } else {
                let data = ProjectileData {
                    homing: true,
                    ..ProjectileData::flying(Num::int(15))
                };
                Projectiles::load_type(&mut game, bolt, &data);
                let projectile = DeliveryData::Projectile {
                    unit_type: DeclaredName::new("bolt").unwrap(),
                    count: NonZeroU8::MIN,
                    spread_deg: Num::ZERO,
                };
                (projectile, "ctx.projectile(caster.pos, target)")
            };
            let shot = ActionData {
                delivery: Some(delivery),
                ..strike()
            };
            let source = format!(
                r#"
                fn on_resolve(ctx, caster, target) {{
                    let next = {call};
                    if next.state.charge != 0 {{ throw "at its type's default"; }}
                    next.state.charge = 3;
                    if next.state.charge != 3 || next.state.mark != 7 {{
                        throw "reads its write back";
                    }}
                    if {fails} {{ throw "fails"; }}
                }}
                "#
            );
            let action = Abilities::load(&mut game, "shot", &shot, &source);
            let caster = Abilities::caster(&mut game, action, 1);
            let target = Abilities::spawn_combatant(&mut game, (3, 0, 0), enemy, ());
            assert_eq!([caster, target].map(StableId::get), [0, 1]);
            Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
            game.run_until(1);
            let mut query = game.query::<(&StableId, &UnitType, &UnitState<SyncNone>)>();
            let mut spawned: Vec<_> = query
                .iter(&game)
                .filter(|&(_, &unit_type, _)| unit_type == bolt)
                .map(|(id, _, state)| (id.get(), state.values().to_vec()))
                .collect();
            spawned.sort_unstable_by_key(|&(id, _)| id);
            let failed = Abilities::failed_calls(&game).len();
            if fails {
                assert_eq!((spawned, failed), (vec![], 1), "{area}");
                let next = Abilities::spawn_combatant(&mut game, (5, 0, 0), enemy, ());
                assert_eq!(next.get(), 2, "{area}");
            } else {
                let both = vec![(2, state(3)), (3, state(0))];
                assert_eq!((spawned, failed), (both, 0), "{area}");
            }
        }
    }
}

/// Fan of Frost as data alone: five arrows of `frost_arrow` over 30 degrees, aimed along a
/// direction, whose hits deal the `damage` param, 30, as physical damage, apply `chilled`, and
/// restore 2 mana to the caster.
fn fan_of_frost() -> ActionData {
    ActionData {
        script: None,
        targeting: Targeting::Direction,
        range: None,
        delivery: Some(DeliveryData::Projectile {
            unit_type: DeclaredName::new("frost_arrow").unwrap(),
            count: NonZeroU8::new(5).unwrap(),
            spread_deg: Num::int(30),
        }),
        on_hit: vec![
            effect(
                DoesData::Damage(DamageFields {
                    amount: param("damage"),
                    kind: DeclaredName::new("physical").unwrap(),
                }),
                EffectTo::Reached,
            ),
            effect(
                DoesData::Modifier(ModifierFields {
                    id: DeclaredName::new("chilled").unwrap(),
                    duration_ms: None,
                }),
                EffectTo::Reached,
            ),
            effect(
                DoesData::Restore(RestoreFields {
                    pool: DeclaredName::new("mana").unwrap(),
                    amount: int(2),
                }),
                EffectTo::Source,
            ),
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
    let mut game = abilities_match_with(
        ScriptLimits::ROOMY,
        &[
            Capability::Stats,
            Capability::Combat,
            Capability::Abilities,
            Capability::Projectiles,
        ],
    );
    Abilities::load_stats(&mut game);
    let chilled = ModifierData {
        duration_ms: Some(int(2000)),
        ..scripted(None, &[])
    };
    let chilled = ModifierData {
        script: None,
        ..chilled
    };
    Stats::load_modifier(&mut game, 0, "chilled", &chilled, None);
    let arrow = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "frost_arrow",
        &UnitTypeCore::default(),
    );
    let data = ProjectileData {
        width: halves(1),
        range: Some(Num::int(6)),
        ..ProjectileData::flying(Num::int(15))
    };
    Projectiles::load_type(&mut game, arrow, &data);
    let fan = fan_of_frost();
    let action = Actions::load(&mut game, 0, "fan_of_frost", &fan, None, 1).unwrap();
    Actions::load_effect_lists(&mut game, action, 0, &fan);
    let caster = Abilities::caster(&mut game, action, 1);
    // Bodiless units: on the middle arrow 4 m out; on the outer arrow 4 m out, at 15 degrees,
    // (3.8637, 1.0353); between two arrows at 4 m, 3.75 degrees off each, 0.26 m from each line,
    // past its 0.25; at 26.6 degrees, past the fan; 8 m out on the middle line, past the range;
    // behind the caster; and an ally of the caster's on the middle line.
    let point = |x: Num, z: Num| Position::try_from(Vec3::from((x, 0, z))).unwrap();
    let decimal = |text: &str| text.parse::<Num>().unwrap();
    let enemies = [
        point(Num::int(4), Num::ZERO),
        point(decimal("3.8637"), decimal("1.0353")),
        point(decimal("3.9914"), decimal("0.2617")),
        point(Num::int(4), Num::int(2)),
        point(Num::int(8), Num::ZERO),
        point(Num::int(-2), Num::ZERO),
    ]
    .map(|pos| Abilities::spawn_combatant(&mut game, pos, Team::new(1), ()));
    let ally =
        Abilities::spawn_combatant(&mut game, point(Num::int(2), Num::ZERO), Team::new(0), ());
    for unit in enemies.iter().chain([&ally]) {
        game.insert_parts(*unit, (Modifiers::default(), ModifierClocks::default()));
    }
    Abilities::cast(
        &mut game,
        caster,
        ActionTarget::Point(point(Num::int(10), Num::ZERO)),
    );
    game.run_until(16);
    let chill = Stats::modifier(&game, 0, "chilled").unwrap();
    let struck = |id: StableId| {
        let entity = game.unit_entity(id);
        let chilled = game
            .get::<Modifiers>(entity)
            .unwrap()
            .get(chill, Some(caster));
        (Stats::health(&game, id), chilled.is_some())
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
    assert_eq!(Abilities::pool(&game, caster), 94);
    assert_eq!(Abilities::failed_calls(&game), []);
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
        let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
        let blast = Units::load_type(
            &mut game,
            TypeScope::Mode,
            "blast",
            &UnitTypeCore::default(),
        );
        let data = AreaData {
            radius: Num::int(2),
            delay_ms,
            duration_ms: 0,
            affects: None,
            inside: AreaInsideData::default(),
        };
        Areas::load_type(&mut game, blast, 0, &data);
        let shot = ActionData {
            targeting: Targeting::Point,
            delivery: Some(DeliveryData::Area {
                unit_type: DeclaredName::new("blast").unwrap(),
            }),
            ..strike()
        };
        // Each hook deals damage only when it reads the area still there, no aimed unit, and no
        // direction, as an area flies none.
        let source = r#"
            fn on_hit(ctx, caster, target, hit) {
                if hit.delivery.unit_type == "blast" && hit.target == () && hit.direction == () {
                    ctx.damage(target, 50, "true");
                }
            }
            fn on_end(ctx, caster, hit) {
                if hit.delivery.unit_type == "blast" {
                    ctx.damage(caster, 7, "true");
                }
            }
        "#;
        let action = Abilities::load(&mut game, "shot", &shot, source);
        let caster = Abilities::caster(&mut game, action, 1);
        // The area lands on (4, 0, 0), of radius 2: a body of no radius 2 m away is inside it and
        // one a bit farther is not; a body of radius 0.5 2.5 m away touches it; an enemy whose
        // tags block it as a target is reached all the same; an ally is not.
        let center = Num::int(4);
        let edge = Abilities::spawn_combatant(
            &mut game,
            Position::try_from(Vec3::from((center + Num::int(2), 0, 0))).unwrap(),
            Team::new(1),
            (),
        );
        let beyond = Abilities::spawn_combatant(
            &mut game,
            Position::try_from(Vec3::from((center, 0, Num::int(2) + Num::EPSILON))).unwrap(),
            Team::new(1),
            (),
        );
        let body = Body::new(halves(1)).unwrap();
        let wide = Abilities::spawn_combatant(
            &mut game,
            Position::try_from(Vec3::from((center - halves(5), 0, 0))).unwrap(),
            Team::new(1),
            body,
        );
        let hidden = Abilities::spawn_combatant(
            &mut game,
            Position::try_from(Vec3::from((center, 0, 1))).unwrap(),
            Team::new(1),
            (),
        );
        Units::set_blocks(&mut game, hidden, &[Block::Target]);
        let ally = Abilities::spawn_combatant(
            &mut game,
            Position::try_from(Vec3::from((center, 0, 0))).unwrap(),
            Team::new(0),
            (),
        );
        let units = [edge, beyond, wide, hidden, ally, caster];
        let areas = |game: &mut Match| {
            let mut query = game.query::<&Area>();
            query.iter(game).count()
        };

        Abilities::cast(
            &mut game,
            caster,
            ActionTarget::Point(Position::try_from(Vec3::from((center, 0, 0))).unwrap()),
        );
        game.run_until(trigger);
        assert_eq!(
            units.map(|id| Stats::health(&game, id)),
            [500; 6],
            "{delay_ms}"
        );
        assert_eq!(areas(&mut game), 1);
        // It triggers, deals 50 to each enemy it reaches, ends at once with no duration, and
        // its `on_end` deals 7 to its caster.
        game.run_until(trigger + 1);
        let healths = units.map(|id| Stats::health(&game, id));
        assert_eq!(healths, [450, 500, 450, 450, 500, 493], "{delay_ms}");
        assert_eq!(areas(&mut game), 0);
        assert_eq!(Abilities::failed_calls(&game), []);
    }
}

#[test]
fn an_area_holds_its_inside_modifiers_by_attitude_and_hits_what_its_filter_selects() {
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Abilities,
        Capability::Areas,
    ];
    let mut game = abilities_match_with(ScriptLimits::ROOMY, &declared);
    let grunt = UnitTypeCore::tagged(&["grunt"]);
    let grunt = Units::load_type(&mut game, TypeScope::Mode, "grunt", &grunt);
    let field = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "field",
        &UnitTypeCore::default(),
    );
    Abilities::load_stats(&mut game);
    let [cover, rally, slow] = ["cover", "rally", "slow"].map(|name| {
        Stats::load_modifier(&mut game, 0, name, &changing(&[], &[], None), None);
        Stats::modifier(&game, 0, name).unwrap()
    });
    // Radius 2 and 100 ms, 3 ticks at 30 a second; it hits only enemy grunts, and holds a
    // modifier of its own on its caster, its caster's allies and its enemies.
    let data = AreaData {
        radius: Num::int(2),
        delay_ms: 0,
        duration_ms: 100,
        affects: Some(FilterData::parse("enemies:grunt").unwrap()),
        inside: AreaInsideData {
            caster: Some(DeclaredName::new("cover").unwrap()),
            allies: Some(DeclaredName::new("rally").unwrap()),
            enemies: Some(DeclaredName::new("slow").unwrap()),
        },
    };
    Areas::load_type(&mut game, field, 0, &data);
    // The tag book, which a match's stats derive each unit's tags from, once every type is tagged.
    Units::seal_types_with(&mut game, &BTreeMap::new());
    let shot = ActionData {
        targeting: Targeting::Point,
        delivery: Some(DeliveryData::Area {
            unit_type: DeclaredName::new("field").unwrap(),
        }),
        ..strike()
    };
    let source = r#"
        fn on_hit(ctx, caster, target, hit) {
            ctx.damage(target, 50, "true");
        }
    "#;
    let action = Abilities::load(&mut game, "shot", &shot, source);
    let caster = Abilities::caster(&mut game, action, 1);
    game.insert_parts(caster, (Modifiers::default(), ModifierClocks::default()));
    // The area lands on (1, 0, 0), with its caster 1 m away. Inside it: an ally, an enemy grunt,
    // an enemy of no type, and one whose tags block it as a target. Outside it, 3 m away: an ally
    // and an enemy grunt.
    let point = |x: i16, z: i16| Position::from((x, 0, z));
    let none = || (Modifiers::default(), ModifierClocks::default());
    let ally = Abilities::spawn_combatant(&mut game, point(1, 1), Team::new(0), none());
    let enemy_grunt = Abilities::spawn_combatant_of(
        &mut game,
        grunt,
        point(2, 0),
        Team::new(1),
        (Level::default(), none()),
    );
    let enemy = Abilities::spawn_combatant(&mut game, point(1, -1), Team::new(1), none());
    let hidden = Abilities::spawn_combatant(&mut game, point(0, 0), Team::new(1), none());
    Units::set_blocks(&mut game, hidden, &[Block::Target]);
    let far_ally = Abilities::spawn_combatant(&mut game, point(4, 0), Team::new(0), none());
    let far_grunt = Abilities::spawn_combatant_of(
        &mut game,
        grunt,
        point(1, 3),
        Team::new(1),
        (Level::default(), none()),
    );
    let units = [
        caster,
        ally,
        enemy_grunt,
        enemy,
        hidden,
        far_ally,
        far_grunt,
    ];
    let held = |game: &Match| units.map(|unit| Stats::carried_modifiers(game, unit));
    let from_caster = |id| vec![(id, Some(caster))];
    let inside = [
        from_caster(cover),
        from_caster(rally),
        from_caster(slow),
        from_caster(slow),
        from_caster(slow),
        vec![],
        vec![],
    ];

    // It lands in tick 0 and holds its modifiers from that tick, on the hidden enemy too. It
    // triggers in tick 1, where its filter decides alone: 50 to the enemy grunt inside, and to no
    // other.
    Abilities::cast(&mut game, caster, ActionTarget::Point(point(1, 0)));
    game.run_until(1);
    assert_eq!(held(&game), inside);
    game.run_until(2);
    let healths = units.map(|unit| Stats::health(&game, unit));
    assert_eq!(healths, [500, 500, 450, 500, 500, 500, 500]);
    // It ends in tick 3, and its modifiers with it.
    game.run_until(3);
    assert_eq!(held(&game), inside);
    game.run_until(4);
    assert_eq!(held(&game), [const { Vec::new() }; 7]);
    assert_eq!(Abilities::failed_calls(&game), []);
}

#[test]
fn a_cast_under_way_ends_when_its_caster_dies() {
    // Strike with a windup of 200 ms, 6 ticks: cast in tick 0, it would resolve in tick 6. The
    // caster dies in tick 2 and comes back in tick 4, before the cast's time, and the cast is
    // gone: the enemy takes nothing, then or later.
    let mut game = abilities_match();
    let windup = ActionData {
        windup_ms: Some(Ranked::One(int(200))),
        ..strike()
    };
    let strike = Abilities::load(&mut game, "strike", &windup, STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
    assert!(Abilities::casting(&game, caster).is_some());
    game.run_until(2);
    let entity = game.unit_entity(caster);
    game.get_mut::<Life>(entity).unwrap().take(Num::int(500));
    game.run_until(3);
    assert!(game.entity(entity).contains::<Dead>());
    assert_eq!(Abilities::casting(&game, caster), None);
    game.entity_mut(entity).remove::<Dead>();
    Abilities::give_pools(&mut game, caster, 100, 20);
    game.run_until(12);
    assert_eq!(Stats::health(&game, enemy), 500);
    assert_eq!(Abilities::pool(&game, caster), 100);
}
