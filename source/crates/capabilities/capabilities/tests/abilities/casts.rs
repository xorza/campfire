use std::collections::BTreeMap;
use std::sync::Arc;

use campfire_abilities::internals::{MANA, RAGE};
use campfire_abilities::*;
use campfire_actions::{
    ActionData, ActionKeys, ActionProgress, ActionRange, ActionSlots, ActionTarget, Actions,
    EffectTo, InProgress, OrderPhase, RangeField, SlotAim, SlotKind, StartedOrder, Targeting,
    TimedSlots,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Release};
use campfire_combat::DamageEffectData;
use campfire_common::{DeclaredName, PlayerSlot, Tick};
use campfire_deliveries::DashDelivering;
use campfire_math::{Num, Vec3};
use campfire_navigation::{MoveEffectData, Navigation, NavigationRefusal};
use campfire_orders::{AiData, Orders};
use campfire_players::PlayersWorld;
use campfire_script::internals::FailedCall;
use campfire_script::{
    ApiError, BudgetOf, CapabilityRefusal, FailureKind, NumError, ScriptBudgets, ScriptLimits,
};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimSystems, SimWorld};
use campfire_stats::{Pools, Stats};
use campfire_units::internals::Spawn;
use campfire_units::{Body, Dead, ForcedMove, Life, MoveStep, Owner, UnitTypeCore, Units};
use campfire_values::{
    ActionStart, Block, DamageKind, FilterData, Named, PackageIndex, PackagePath, Param, Rank,
    Ranked, ResourceId, ScriptSource, Team, TypeScope,
};

use crate::abilities::{
    LASH_OUT, STRIKE, abilities_match, cost, effect, half_meters, int, lash_out,
};

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
    let near = Spawn::named(&game, "combatant", (3, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Abilities::cast(&mut game, husk, ActionTarget::None);
    // The call fails, so the cast applies nothing: no damage, no cost.
    assert_eq!(
        (Stats::health(&game, near), Abilities::pool(&game, husk)),
        (500, 100)
    );
    let failed = FailedCall {
        id: Some(husk),
        hook: Release::hook("on_resolve"),
        kind: FailureKind::Api(ApiError::Unknown(DamageKind::NAMES)),
    };
    assert_eq!(Abilities::failed_calls(&mut game), [failed]);
}

#[test]
fn lash_out_hits_every_enemy_within_its_radius_exactly() {
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    // Strike's one param name comes first in the frame, so Lash Out's three follow from the
    // second.
    Abilities::load(&mut game, "strike", &Abilities::strike(), STRIKE);
    let lash_out = Abilities::load(&mut game, "lash_out", &lash_out(), LASH_OUT);
    let husk = Abilities::caster(&mut game, lash_out, 2);
    let near = Spawn::named(&game, "combatant", (3, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    // At exactly 3.5 m: within the radius.
    let edge = Spawn::named(&game, "combatant", Vec3::from((0, 0, half_meters(7))))
        .team(Team::new(1))
        .place(&mut game);
    let beyond = Spawn::named(&game, "combatant", (4, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    // Up at y = 9, 2 m away on the ground plane.
    let high = Spawn::named(&game, "combatant", (0, 9, 2))
        .team(Team::new(1))
        .place(&mut game);
    let ally = Spawn::named(&game, "combatant", (1, 0, 0))
        .team(Team::new(0))
        .place(&mut game);
    let dead = Spawn::named(&game, "combatant", (1, 0, 1))
        .team(Team::new(1))
        .place(&mut game);
    game.insert_parts(dead, Dead);

    // Rank 2 deals 100, and 0.5 × 0 ability power: 500 → 400 for the three enemies in reach.
    // It costs 35 of 100, and its 9000 ms cooldown is 9 × 30 = 270 ticks.
    Abilities::cast(&mut game, husk, ActionTarget::None);
    let healths =
        |game: &Match| [near, edge, beyond, high, ally, dead].map(|id| Stats::health(game, id));
    assert_eq!(healths(&game), [400, 400, 500, 400, 500, 500]);
    assert_eq!(Abilities::pool(&game, husk), 65);
    assert_eq!(Abilities::time(&game, husk).ready_at, Tick::new(270));
    assert_eq!(Abilities::failed_calls(&mut game), []);

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
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    // Aim winds up 300 ms, 9 ticks, within 10 m; Shove, at once, knocks its target 3 m away from
    // its caster over 100 ms, 3 ticks.
    let aim = ActionData {
        keys: ActionKeys {
            windup_ms: Some(Ranked::One(int(300))),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::int(10),
            )))),
            ..Abilities::strike().keys
        },
        ..Abilities::strike()
    };
    let aim = Abilities::load(&mut game, "aim", &aim, STRIKE);
    let shove = ActionData {
        keys: ActionKeys {
            script: Some(PackagePath::parse("shove.rhai").unwrap()),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::int(5),
            )))),
            ..ActionData::of(
                Targeting::Unit(FilterData::parse("enemies").unwrap()),
                CastData::default(),
            )
            .keys
        },
        ..ActionData::of(
            Targeting::Unit(FilterData::parse("enemies").unwrap()),
            CastData::default(),
        )
    };
    let script =
        "fn on_resolve(ctx, caster, target) { ctx.knock_back(target, caster.position, 3, 100); }";
    let shove = Abilities::load(&mut game, "shove", &shove, script);
    let caster = Abilities::caster(&mut game, aim, 1);
    game.insert_parts(caster, Navigation::walker(MoveStep::new(Num::ONE).unwrap()));
    let slots = TimedSlots::new(ActionSlots::new([(shove, SlotKind::new(0), Rank::new(1))]));
    let shover = Spawn::named(&game, "combatant", (5, 0, 0))
        .team(Team::new(1))
        .parts(slots)
        .place(&mut game);

    // Aim starts in tick 0, to resolve in tick 9. Shove resolves in tick 2's Hit stage, after
    // Move, and knocks the caster along −x in ticks 3, 4 and 5: Aim's windup is interrupted in
    // tick 3, and its order waits through tick 5. It starts again in tick 6, from (−3, 0, 0),
    // and resolves in tick 15.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(shover));
    game.step();
    Abilities::cast(&mut game, shover, ActionTarget::Unit(caster));
    game.run_until(6);
    assert_eq!(game.part::<Position>(caster).get(), Vec3::from((-3, 0, 0)));
    let ordered = InProgress::Order {
        aim: SlotAim {
            slot: 0,
            target: ActionTarget::Unit(shover),
        },
        phase: OrderPhase::Ordered,
    };
    assert_eq!(
        game.part::<ActionProgress>(caster).in_progress(),
        Some(ordered)
    );
    game.run_until(15);
    assert_eq!(Stats::health(&game, shover), 500);
    game.step();
    assert_eq!(Stats::health(&game, shover), 450);
    assert_eq!(Abilities::failed_calls(&mut game), []);

    // A forced move of a unit that does not walk, a dash of no step, a knock back of no
    // distance or of no time, fails.
    let refused = [
        (
            shover,
            "ctx.teleport(of, of.position)",
            NavigationRefusal::NoWalker.refused(),
        ),
        (
            caster,
            "ctx.dash(of, of.position, 0)",
            NavigationRefusal::NotASpeed.refused(),
        ),
        (
            caster,
            "ctx.dash(of, of.position, -1)",
            NavigationRefusal::NotASpeed.refused(),
        ),
        (
            caster,
            "ctx.knock_back(of, of.position, 0, 100)",
            NavigationRefusal::NotADistance.refused(),
        ),
        (
            caster,
            "ctx.knock_back(of, of.position, 1, 0)",
            ApiError::ZeroTime,
        ),
        (
            caster,
            "ctx.knock_back(of, of.position, 1, -1)",
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
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    // Lunge knocks the unit it reaches 3 m away from its caster over 100 ms, 3 ticks, and dashes
    // its caster at it, 30 m a second, a meter a tick. It delivers at once, so the dash is its
    // delivery: as the dash ends, its `on_end` list deals its caster 1, and its hook deals the
    // dash's target the meters it went and ten times how far from where it began it ended, when
    // no projectile or area delivered it and its last step went somewhere.
    let knock_back = MoveEffectData::KnockBack {
        from: EffectTo::Source,
        distance: int(3),
        ms: int(100),
    };
    let dash = MoveEffectData::Dash {
        to: EffectTo::Reached,
        speed: int(30),
    };
    let damage = Arc::new(DamageEffectData {
        amount: int(1),
        kind: DeclaredName::new("true").unwrap(),
    });
    let lunge = ActionData {
        keys: ActionKeys {
            script: Some(PackagePath::parse("lunge.rhai").unwrap()),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::int(10),
            )))),
            ..ActionData::of(
                Targeting::Unit(FilterData::parse("enemies").unwrap()),
                CastData::default(),
            )
            .keys
        },
        on_resolve: vec![
            effect(Arc::new(knock_back), EffectTo::Reached),
            effect(Arc::new(dash), EffectTo::Source),
        ],
        on_end: vec![effect(damage, EffectTo::Source)],
        ..ActionData::of(
            Targeting::Unit(FilterData::parse("enemies").unwrap()),
            CastData::default(),
        )
    };
    let source = r#"
        fn on_end(ctx, caster, hit) {
            if hit.delivery == () && hit.direction != () {
                let from = hit.position.distance_to(ctx.origin);
                ctx.damage(hit.aimed, "true", hit.distance + from * 10);
            }
        }
    "#;
    let script = Units::compile_hooked(&mut game, source).unwrap();
    let id = Actions::load(
        &mut game,
        PackageIndex::MODE,
        "lunge",
        &lunge,
        Some(script),
        1,
    )
    .unwrap();
    Actions::load_effect_lists(&mut game, id, PackageIndex::MODE, &lunge);
    let lunge = id;
    let walker = || Navigation::walker(MoveStep::new(Num::ONE).unwrap());
    let caster = Abilities::caster(&mut game, lunge, 1);
    game.insert_parts(caster, walker());
    let target = Spawn::named(&game, "combatant", (2, 0, 0))
        .team(enemy)
        .parts(walker())
        .place(&mut game);
    // It resolves in tick 0. The caster, of the lower id, moves first each tick, at the target's
    // place before the target's own move: 1, 2, 3, then 4, and in tick 5 onto 5, where they
    // touch, as neither has a body. The target goes 3, 4, 5.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
    let mut places = Vec::new();
    let mut healths = Vec::new();
    for _ in 0..5 {
        game.step();
        let x = |id| game.part::<Position>(id).x.to_int().unwrap();
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
    assert_eq!(Abilities::failed_calls(&mut game), []);
    // A unit that does not walk takes no forced move: the list fails the cast.
    let still = Spawn::named(&game, "combatant", (6, 0, 0))
        .team(enemy)
        .place(&mut game);
    game.run_until(40);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(still));
    let failed = Abilities::failed_calls(&mut game);
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].kind,
        FailureKind::Api(NavigationRefusal::NoWalker.refused())
    );
    // A dash a knock back cuts delivers nothing, as League of Legends' and Dota 2's cut dashes
    // deliver nothing: the knock back replaces it, and its end runs no `on_end`.
    let far = Spawn::named(&game, "combatant", (12, 0, 0))
        .team(enemy)
        .parts(walker())
        .place(&mut game);
    game.run_until(80);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(far));
    let dashing = game.part::<ForcedMove>(caster);
    assert!(matches!(dashing, ForcedMove::Dash { .. }));
    // It delivers from where the last dash ended, (5, 0, 0), and went nowhere yet.
    let delivering = game.try_part::<DashDelivering>(caster);
    let delivering = delivering.map(|dash| (dash.by.start.map(|start| start.origin), dash.dashed));
    assert_eq!(delivering, Some((Some(Vec3::from((5, 0, 0))), Num::ZERO)));
    let cut = ForcedMove::KnockBack {
        to: Vec3::ZERO,
        left: 2,
    };
    // A knock back takes the place of the dash, and its delivery with it, as navigation's effect
    // puts one.
    let entity = game.unit_entity(caster);
    game.entity_mut(entity)
        .insert(cut)
        .remove::<DashDelivering>();
    game.run_until(120);
    assert_eq!(game.part::<Position>(caster).x, Num::ZERO);
    assert_eq!(
        (Stats::health(&game, caster), Stats::health(&game, far)),
        (499, 500)
    );
    assert_eq!(Abilities::failed_calls(&mut game), []);
}

#[test]
fn ai_load_does_not_spend_what_a_cast_needs() {
    // The one test of how abilities and `orders` meet: AI's calls and a cast's draw from budgets of
    // their own.
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Orders,
    ];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    // Eleven units whose AI spins, all due in every tick: ten calls fail at the 10 000 limit and
    // spend the 100 000 of the think budget, and the eleventh finds it spent. Their type loads
    // before any unit spawns, as a match's types do.
    let spinner = Units::load_type(
        &mut game,
        TypeScope::Mode,
        "spinner",
        &UnitTypeCore::default(),
    );
    Abilities::give_combatant(&mut game, spinner);
    let ai = AiData {
        ai: ScriptSource::Own(PackagePath::parse("scripts/ai.rhai").unwrap()),
        think_ms: 1,
    };
    let spin = "fn on_think(ctx, unit) { loop {} }";
    let spin = Units::compile_hooked(&mut game, spin).unwrap();
    Orders::load_ai(&mut game, spinner, &ai, spin).unwrap();
    let strike = Abilities::load(&mut game, "strike", &Abilities::strike(), STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy = Spawn::named(&game, "combatant", (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let spinners: Vec<_> = (0..11)
        .map(|z| {
            Spawn::of(spinner, (0, 0, 20 + z))
                .team(Team::new(2))
                .place(&mut game)
        })
        .collect();

    // The cast in the same tick draws from its player's budget, whole: 500 → 450, 100 → 90.
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
    assert_eq!(Stats::health(&game, enemy), 450);
    assert_eq!(Abilities::pool(&game, caster), 90);
    // The think budget runs out after 10 of the spinning calls: the first 10 spinners, by stable
    // id, spend it, and the last stays due.
    let expected: Vec<_> = spinners[..10]
        .iter()
        .map(|&id| FailedCall {
            id: Some(id),
            hook: Release::hook("on_think"),
            kind: FailureKind::CallLimit,
        })
        .collect();
    assert_eq!(Abilities::failed_calls(&mut game), expected);
    let mut budgets = game.resource_mut::<ScriptBudgets>();
    assert_eq!(budgets.get_mut(BudgetOf::Think).left(), 0);
}

#[test]
fn a_cast_passes_its_checks_or_does_nothing() {
    let mut game = abilities_match();
    let strike = Abilities::load(&mut game, "strike", &Abilities::strike(), STRIKE);
    // Casters of 5 mana, of 3 rage, and of 10 mana and 4 rage, beside the 100 and 20 of the
    // match's own.
    let [poor, calm, exact] = [(5, 20), (100, 3), (10, 4)].map(|(mana, rage)| {
        let unit_type = Units::load_next_type(&mut game, &UnitTypeCore::default());
        Abilities::give_caster(&mut game, unit_type, mana, rage);
        unit_type
    });
    let full = Units::type_named(&game, "caster").unwrap();
    let caster = Abilities::caster(&mut game, strike, 1);
    let mut caster_at = |x: i16, rank: u8, unit_type| {
        let parts = (
            Owner::new(PlayerSlot::new(0)),
            TimedSlots::new(ActionSlots::new([(
                strike,
                SlotKind::new(0),
                Rank::new(rank),
            )])),
        );
        Spawn::of(unit_type, (x, 0, 0))
            .parts(parts)
            .place(&mut game)
    };
    let unlearned = caster_at(1, 0, full);
    let poor = caster_at(2, 1, poor);
    let calm = caster_at(2, 1, calm);
    let spent = caster_at(2, 1, full);
    let exact = caster_at(2, 1, exact);
    let entity = game.unit_entity(spent);
    game.get_mut::<Pools>(entity)
        .unwrap()
        .take(MANA, Num::int(91));
    let enemy = Spawn::named(&game, "combatant", (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let far = Spawn::named(&game, "combatant", (6, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let ally = Spawn::named(&game, "combatant", (1, 0, 1))
        .team(Team::new(0))
        .place(&mut game);
    let hidden = Spawn::named(&game, "combatant", (1, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
    let gold = ResourceId::new(0);
    let mut cost = cost("mana", int(10));
    cost.insert(DeclaredName::new("gold").unwrap(), Ranked::One(int(30)));
    let data = ActionData {
        keys: ActionKeys {
            cost,
            ..Abilities::strike().keys
        },
        ..Abilities::strike()
    };
    let strike = Abilities::load(&mut game, "strike", &data, STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let slots = TimedSlots::new(ActionSlots::new([(strike, SlotKind::new(0), Rank::new(1))]));
    let ownerless = Spawn::named(&game, "caster", (0, 0, 1))
        .parts(slots)
        .place(&mut game);
    let enemy = Spawn::named(&game, "combatant", (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
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
        keys: ActionKeys {
            windup_ms: Some(Ranked::One(int(100))),
            ..Abilities::strike().keys
        },
        ..Abilities::strike()
    };
    let strike = Abilities::load(&mut game, "strike", &data, STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy = Spawn::named(&game, "combatant", (5, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    let target = ActionTarget::Unit(enemy);
    let aim = SlotAim { slot: 0, target };
    let ordered = Some(InProgress::Order {
        aim,
        phase: OrderPhase::Ordered,
    });
    // Each start from where the caster stands, the origin.
    let start = ActionStart {
        origin: Vec3::from((0, 0, 0)),
        charge: None,
    };
    let started = |tick| {
        Some(InProgress::Order {
            aim,
            phase: OrderPhase::Started(StartedOrder {
                resolves_at: Tick::new(tick),
                start,
            }),
        })
    };
    // A stun in tick 7's Move stage, after the casts start in Act and before they resolve in Hit.
    let stunned = &[Block::Move, Block::Attack, Block::Cast, Block::Use];
    Units::block_at(&mut game, caster, 7, SimSystems::Move, stunned);

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
    let spin = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.position, 10, "enemies") { ctx.damage(unit, "magic", 50); } loop {} }"#;
    let wrong_kind = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.position, 10, "enemies") { ctx.damage(unit, "fire", 50); } }"#;
    let overflow = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.position, 10, "enemies") { ctx.damage(unit, "magic", 50); } num(1 << 20) * num(1 << 20) }"#;
    let undeclared = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.position, 10, "enemies") { ctx.damage(unit, "magic", 50); } ctx.params.radius }"#;
    let data = ActionData {
        keys: ActionKeys {
            params: BTreeMap::new(),
            ..lash_out().keys
        },
        ..lash_out()
    };
    let thrown = r#"fn on_resolve(ctx, caster, target) { for unit in ctx.find(caster, caster.position, 10, "enemies") { ctx.damage(unit, "magic", 50); } throw "out" }"#;
    let cases = [
        (spin, FailureKind::CallLimit),
        (
            wrong_kind,
            FailureKind::Api(ApiError::Unknown(DamageKind::NAMES)),
        ),
        (
            undeclared,
            FailureKind::Api(ApiError::Unknown(Param::NAMES)),
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
            let enemy = Spawn::named(&game, "combatant", (1, 0, 0))
                .team(Team::new(1))
                .place(&mut game);
            if ordered {
                Abilities::cast(&mut game, caster, ActionTarget::None);
                let failed = FailedCall {
                    id: Some(caster),
                    hook: Release::hook("on_resolve"),
                    kind: expected,
                };
                assert_eq!(Abilities::failed_calls(&mut game), [failed], "{script}");
                let mut budgets = game.resource_mut::<ScriptBudgets>();
                spent.push(
                    ScriptLimits::ROOMY.player
                        - budgets.get_mut(BudgetOf::Player(PlayerSlot::new(0))).left(),
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
fn a_cast_under_way_ends_when_its_caster_dies() {
    // Strike with a windup of 200 ms, 6 ticks: cast in tick 0, it would resolve in tick 6. The
    // caster dies in tick 2 and comes back in tick 4, before the cast's time, and the cast is
    // gone: the enemy takes nothing, then or later.
    let mut game = abilities_match();
    let windup = ActionData {
        keys: ActionKeys {
            windup_ms: Some(Ranked::One(int(200))),
            ..Abilities::strike().keys
        },
        ..Abilities::strike()
    };
    let strike = Abilities::load(&mut game, "strike", &windup, STRIKE);
    let caster = Abilities::caster(&mut game, strike, 1);
    let enemy = Spawn::named(&game, "combatant", (3, 0, 0))
        .team(Team::new(1))
        .place(&mut game);
    Abilities::cast(&mut game, caster, ActionTarget::Unit(enemy));
    let started = InProgress::Order {
        aim: SlotAim {
            slot: 0,
            target: ActionTarget::Unit(enemy),
        },
        phase: OrderPhase::Started(StartedOrder {
            resolves_at: Tick::new(6),
            start: ActionStart {
                origin: Vec3::ZERO,
                charge: None,
            },
        }),
    };
    assert_eq!(Abilities::casting(&game, caster), Some(started));
    game.run_until(2);
    let entity = game.unit_entity(caster);
    game.get_mut::<Life>(entity).unwrap().take(Num::int(500));
    game.run_until(3);
    assert!(game.entity(entity).contains::<Dead>());
    assert_eq!(Abilities::casting(&game, caster), None);
    game.entity_mut(entity).remove::<Dead>();
    game.entity_mut(entity).insert(Life::full(Num::int(500)));
    game.run_until(12);
    assert_eq!(Stats::health(&game, enemy), 500);
    assert_eq!(Abilities::pool(&game, caster), 100);
}
