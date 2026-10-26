use std::collections::BTreeMap;
use std::num::NonZeroU8;

use campfire_abilities::*;
use campfire_actions::internals::LooseDues;
use campfire_actions::{
    ActionBook, ActionData, ActionKeys, ActionProgress, ActionRange, ActionSlots, ActionTarget,
    Actions, ChannelData, ChannelPaysData, ChargeRule, ChargeUpData, ChargesData, InProgress,
    OrderPhase, ProgressDues, RangeField, SlotKind, SlotTimes, SlotsMut, Targeting, TimedSlots,
    Toggle,
};
use campfire_capabilities::internals::*;
use campfire_capabilities::{Match, Order, Release};
use campfire_combat::Combat;
use campfire_common::{DeclaredName, PlayerSlot, Tick, Ticks};
use campfire_declarations::Role;
use campfire_declarations::internals::BookLoad;
use campfire_math::{Num, Rng, RngStream, Vec3};
use campfire_navigation::{Navigation, Paths};
use campfire_orders::{HeldOrder, OrdersDirective};
use campfire_players::PlayersWorld;
use campfire_progression::{Experience, TrackBook, TrackSet};
use campfire_script::FailureKind;
use campfire_script::internals::FailedCall;
use campfire_script::{CapabilityRefusal, Pool, ScriptBudgets, ScriptLimits};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Capability, Position, SimComponent, SimRng, SimWorld, StableId};
use campfire_stats::{ModifierData, Modifiers, Stats};
use campfire_units::internals::Spawn;
use campfire_units::{GroundPoint, Life, Owner, UnitTypeCore, Units};
use campfire_values::{
    ActionStart, Block, FilterData, Number, PackageIndex, PackagePath, Rank, Ranked, ResourceId,
    Scalar, Team, TrackId,
};

use crate::abilities::{LASH_OUT, STRIKE, abilities_match, cost, int, lash_out};

#[test]
fn charges_are_spent_one_a_cast_and_come_back_one_at_a_time() {
    // With combat and without, as a cast draws its charges in any match.
    charges_come_back(&[Capability::Stats, Capability::Combat, Capability::Abilities]);
    charges_come_back(&[Capability::Stats, Capability::Abilities]);
}

/// The charges of a match of `declared`, as the test says.
fn charges_come_back(declared: &[Capability]) {
    // Step: 3 charges, one back each 1000 ms, 30 ticks at 30 a second, and a lockout of 100 ms,
    // 3 ticks, between two casts; 10 mana a cast. Refill gives Step a charge and takes 500 ms,
    // 15 ticks, off the time to its next; Dud has no charges to give.
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, declared);
    Abilities::load_stats(&mut game);
    let plain = ActionData {
        keys: ActionKeys {
            script: Some(PackagePath::parse("cast.rhai").unwrap()),
            ..ActionData::of(Targeting::None, CastData::default()).keys
        },
        ..ActionData::of(Targeting::None, CastData::default())
    };
    let step = ActionData {
        keys: ActionKeys {
            charges: Some(ChargesData {
                max: Ranked::One(int(3)),
                recharge_ms: Ranked::One(int(1000)),
            }),
            cooldown_ms: Some(Ranked::One(int(100))),
            cost: cost("mana", int(10)),
            ..plain.clone().keys
        },
        ..plain.clone()
    };
    let idle = "fn on_resolve(ctx, caster, target) {}";
    let step = Abilities::load(&mut game, "step", &step, idle);
    let refill = r#"fn on_resolve(ctx, caster, target) {
        ctx.add_charge(caster, "step");
        ctx.cut_cooldown(caster, "step", 500);
    }"#;
    let refill = Abilities::load(&mut game, "refill", &plain, refill);
    let dud = r#"fn on_resolve(ctx, caster, target) { ctx.add_charge(caster, "dud"); }"#;
    let dud = Abilities::load(&mut game, "dud", &plain, dud);
    let caster = Spawn::named(&game, "caster", Vec3::ZERO)
        .parts((
            Owner::new(PlayerSlot::new(0)),
            TimedSlots::new(ActionSlots::new([
                (step, SlotKind::new(0), Rank::new(1)),
                (refill, SlotKind::new(0), Rank::new(1)),
                (dud, SlotKind::new(0), Rank::new(1)),
            ])),
        ))
        .place(&mut game);
    let charges = |game: &Match| {
        let charges = Abilities::time(game, caster).charges.unwrap();
        (charges.count, charges.next.get())
    };
    let held = |game: &Match| (charges(game), Abilities::pool(game, caster));
    let order = |game: &mut Match, slot| {
        ProgressDues::change_unit(game, caster, |progress, dues| {
            progress.order(slot, ActionTarget::None, dues);
        });
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
    assert_eq!(Abilities::failed_calls(&mut game), []);
    // Refill at full adds none; Dud names an ability with no charges, and fails.
    order(&mut game, 1);
    assert_eq!(charges(&game).0, 3);
    order(&mut game, 2);
    let failed = Abilities::failed_calls(&mut game);
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].kind,
        FailureKind::Api(AbilitiesRefusal::NoCharges.refused())
    );
    // A unit that holds Step unlearned has no charges for Refill to add to.
    let learner = Spawn::named(&game, "caster", (5, 0, 0))
        .parts((
            Owner::new(PlayerSlot::new(0)),
            TimedSlots::new(ActionSlots::new([
                (step, SlotKind::new(0), Rank::new(0)),
                (refill, SlotKind::new(0), Rank::new(1)),
            ])),
        ))
        .place(&mut game);
    ProgressDues::change_unit(&mut game, learner, |progress, dues| {
        progress.order(1, ActionTarget::None, dues);
    });
    game.step();
    let unlearned = game.part::<ActionSlots>(learner).slot(0).unwrap();
    let time = game.part::<SlotTimes>(learner).slot(0).unwrap();
    assert_eq!((unlearned.rank, time.charges), (None, None));
    assert_eq!(Abilities::failed_calls(&mut game), []);
    // A slot gives its charges from the book, in a match with no progression: Step's 3 a rank,
    // back in 30 ticks; none for Refill, which has none, nor for Step unlearned.
    let book = game.resource::<ActionBook>();
    let slots = game.part::<ActionSlots>(caster);
    let rule = ChargeRule {
        max: NonZeroU8::new(3).unwrap(),
        recharge: Ticks::new(30),
    };
    let [step, refill] = [0, 1].map(|slot| slots.slot(slot).unwrap().charge_rule(book));
    assert_eq!((step, refill), (Some(rule), None));
    assert_eq!(unlearned.charge_rule(book), None);
}

#[test]
fn a_client_predicts_a_resolved_casts_slot_as_the_server_resolves_it() {
    // One caster casts, a tick each from tick 0: Step, of 3 charges back in 1000 ms, 30 ticks,
    // and a cooldown of 100 ms, 3 ticks; Aura, a toggle that pays each second; then Drain, a
    // channel of 300 ms that ticks each 100 ms. The server resolves each cast, and a client,
    // with no script, predicts it: after each tick their slots are alike.
    let declared = [Capability::Stats, Capability::Combat, Capability::Abilities];
    let cast = || ActionData::of(Targeting::None, CastData::default());
    let step = ActionData {
        keys: ActionKeys {
            charges: Some(ChargesData {
                max: Ranked::One(int(3)),
                recharge_ms: Ranked::One(int(1000)),
            }),
            cooldown_ms: Some(Ranked::One(int(100))),
            ..cast().keys
        },
        ..cast()
    };
    let aura = ActionData {
        keys: ActionKeys {
            toggle: Some(Toggle::CostPerSecond(BTreeMap::new())),
            ..cast().keys
        },
        ..cast()
    };
    let drain = ActionData {
        keys: ActionKeys {
            channel: Some(ChannelData {
                duration_ms: Some(Ranked::One(int(300))),
                tick_ms: Ranked::One(int(100)),
                damage_interrupts: false,
                windup_spread: None,
                wind_down_ms: None,
                pays: None,
            }),
            ..cast().keys
        },
        ..cast()
    };
    let caster = |game: &mut Match| {
        let actions = [("step", &step), ("aura", &aura), ("drain", &drain)];
        let slots = actions.map(|(name, data)| {
            let action = Actions::load(game, PackageIndex::MODE, name, data, None, 1).unwrap();
            (action, SlotKind::new(0), Rank::new(1))
        });
        let unit_type = Units::load_next_type(game, &UnitTypeCore::default());
        let slots = TimedSlots::new(ActionSlots::new(slots));
        Spawn::of(unit_type, Vec3::ZERO).parts(slots).place(game)
    };
    let mut server = Match::builder(&declared).build();
    let mut client = Match::builder(&declared).role(Role::Rules).build();
    let on_server = caster(&mut server);
    let on_client = caster(&mut client);
    for slot in 0..3 {
        for (game, id) in [(&mut server, on_server), (&mut client, on_client)] {
            ProgressDues::change_unit(game, id, |progress, dues| {
                progress.order(slot, ActionTarget::None, dues);
            });
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
    let times = server.part::<SlotTimes>(on_server);
    let step = times.slot(0).unwrap();
    let charges = step.charges.unwrap();
    assert_eq!(
        (charges.count, charges.next, step.ready_at),
        (2, Tick::new(30), Tick::new(3))
    );
    assert_eq!(times.slot(1).unwrap().toggle, Some(Tick::new(31)));
    let progress = server.part::<ActionProgress>(on_server);
    let Some(InProgress::Channel { next, ends, .. }) = progress.in_progress() else {
        panic!("Drain channels");
    };
    assert_eq!((next, ends), (Tick::new(6), Some(Tick::new(12))));
}

#[test]
fn a_channel_ticks_from_the_tick_after_its_cast_and_an_order_a_stun_a_death_or_its_slot_emptied_cuts_it()
 {
    // Drain: a channel of 300 ms, 9 ticks at 30 a second, that ticks each 100 ms, 3 ticks, at
    // its first rank, each 50 ms at its later ones, and holds Ward, a shield of 100. Each tick
    // strikes its target for 10; a cut one strikes it for 7.
    let script = r#"
fn on_channel_tick(ctx, caster, target) {
    ctx.damage(target, "true", 10);
}
fn on_interrupt(ctx, caster, target) {
    ctx.damage(target, "true", 7);
}
"#;
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let ward = ModifierData {
        shield: Some(int(100)),
        ..ModifierData::default()
    };
    Stats::load_modifier(&mut game, PackageIndex::MODE, "ward", &ward, None);
    let drain = ActionData {
        keys: ActionKeys {
            script: Some(PackagePath::parse("drain.rhai").unwrap()),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::int(5),
            )))),
            channel: Some(ChannelData {
                duration_ms: Some(Ranked::One(int(300))),
                tick_ms: Ranked::PerRank(vec![int(100), int(50), int(50), int(50), int(50)]),
                damage_interrupts: false,
                windup_spread: None,
                wind_down_ms: None,
                pays: None,
            }),
            hold: Some(DeclaredName::new("ward").unwrap()),
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
    let drain = Abilities::load(&mut game, "drain", &drain, script);
    let ward = Stats::modifier(&game, PackageIndex::MODE, "ward").unwrap();
    // Cast in tick 0 by a caster at x, an enemy beside it; then `cut` in tick 5, if any.
    let run = |game: &mut Match, x: i16, cut: &dyn Fn(&mut Match, StableId)| {
        let caster = Abilities::caster(game, drain, 1);
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
        ProgressDues::change_unit(game, caster, |progress, dues| {
            progress.order(0, ActionTarget::None, dues);
        });
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
        ProgressDues::with(game, caster, entity, |game, dues| {
            let mut unit = game.entity_mut(entity);
            let mut held = unit.get_components_mut::<SlotsMut>().unwrap();
            held.fill(0, None, dues);
        });
    });
    assert_eq!(health(&emptied), cut);
    // A rank learned in tick 5 leaves the channel at the rank it began with: it ticks each
    // 100 ms on, as a whole one does, not each 50 ms; and the match as it stands then, with its
    // slot past the channel's rank, restores.
    let learned = run(&mut game, 100, &|game, caster| {
        game.part_mut::<ActionSlots>(caster).learn(0);
        let mut snapshot = Vec::new();
        game.snapshot(&mut snapshot);
        game.restore_in_place(&snapshot).unwrap();
    });
    assert_eq!(health(&learned), health(&whole));
    // An order of a slot an item fills afresh in its tick ends, as what it ordered is gone: it
    // does not go back to its order and start what the slot holds then.
    let caster = Abilities::caster(&mut game, drain, 1);
    *game.part_mut::<Position>(caster) = Position::from((120, 0, 0));
    let target = Abilities::spawn_combatant(&mut game, (121, 0, 0), Team::new(1), ());
    ProgressDues::change_unit(&mut game, caster, |progress, dues| {
        progress.order(0, ActionTarget::Unit(target), dues);
    });
    let entity = game.unit_entity(caster);
    ProgressDues::with(&mut game, caster, entity, |game, dues| {
        let mut unit = game.entity_mut(entity);
        let mut held = unit.get_components_mut::<SlotsMut>().unwrap();
        held.fill(0, Some(drain), dues);
    });
    game.step();
    assert_eq!(game.part::<ActionProgress>(caster).in_progress(), None);
}

#[test]
fn a_charged_cast_resolves_at_its_release_or_full_with_its_share_and_its_origin() {
    // Draw: charges up to 3000 ms, 90 ticks, for 10 mana at its release. It deals its target its
    // share times 90, and ten times how far the caster stands from where it started.
    let script = r#"
fn on_resolve(ctx, caster, target) {
    ctx.damage(target, "true", ctx.charged * 90 + ctx.origin.distance_to(caster.position) * 10);
}
"#;
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let draw = ActionData {
        keys: ActionKeys {
            script: Some(PackagePath::parse("draw.rhai").unwrap()),
            range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
                Num::int(5),
            )))),
            charging: Some(ChargeUpData {
                max_ms: Ranked::One(int(3000)),
            }),
            cost: cost("mana", int(10)),
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
    let plain = ActionData {
        keys: ActionKeys {
            charging: None,
            ..draw.clone().keys
        },
        ..draw.clone()
    };
    let draw = Abilities::load(&mut game, "draw", &draw, script);
    let plain = Abilities::load(&mut game, "plain", &plain, script);
    let other = Abilities::load(&mut game, "other", &lash_out(), LASH_OUT);
    let caster = |game: &mut Match, x: i16| {
        let position = Vec3::from((x, 0, 0));
        let slots = TimedSlots::new(ActionSlots::new([
            (draw, SlotKind::new(0), Rank::new(1)),
            (other, SlotKind::new(0), Rank::new(1)),
        ]));
        let caster = Abilities::spawn_caster(
            game,
            position,
            Team::new(0),
            (Owner::new(PlayerSlot::new(0)), slots),
        );
        let target = Abilities::spawn_combatant(game, (x + 3, 0, 0), Team::new(1), ());
        (caster, target)
    };
    let order = |game: &mut Match, id: StableId, slot: u8, target: ActionTarget| {
        ProgressDues::change_unit(game, id, |progress, dues| {
            progress.order(slot, target, dues);
        });
    };
    let run_to = |game: &mut Match, tick: u64| game.run_until(tick);
    let share = |held: i64| Num::int(held) / Num::int(90);

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
    assert_eq!(Abilities::failed_calls(&mut game), []);
    // The same script in an action that does not charge fails its call.
    let position = Vec3::from((100, 0, 0));
    let slots = TimedSlots::new(ActionSlots::new([(plain, SlotKind::new(0), Rank::new(1))]));
    let caster = Abilities::spawn_caster(
        &mut game,
        position,
        Team::new(0),
        (Owner::new(PlayerSlot::new(0)), slots),
    );
    let target = Abilities::spawn_combatant(&mut game, (103, 0, 0), Team::new(1), ());
    Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
    let failed = Abilities::failed_calls(&mut game);
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].kind,
        FailureKind::Api(AbilitiesRefusal::NotCharged.refused())
    );

    // A restored start holds a charge, from 0 to 1, exactly when its action charges at its rank,
    // as `ctx.charge` reads one.
    let entity = game.unit_entity(caster);
    let checked = |action, charge| {
        let slots = ActionSlots::new([(action, SlotKind::new(0), Rank::new(1))]);
        let mut progress = ActionProgress::default();
        let mut loose = LooseDues::default();
        progress.order(0, ActionTarget::Unit(target), &mut loose.dues());
        let start = ActionStart {
            origin: Vec3::ZERO,
            charge,
        };
        let target = ActionTarget::Unit(target);
        progress.schedule(Tick::new(5), target, start, &mut loose.dues());
        slots.check(&game, entity) && progress.check_against(&slots, &game)
    };
    let past = [Num::ONE + Num::EPSILON, Num::ZERO - Num::EPSILON];
    assert_eq!(
        [Num::ZERO, Num::ONE].map(|charge| checked(draw, Some(charge))),
        [true; 2]
    );
    assert!(checked(plain, None));
    assert!(!checked(draw, None) && !checked(plain, Some(Num::ZERO)));
    assert_eq!(past.map(|charge| checked(draw, Some(charge))), [false; 2]);
}

#[test]
fn a_cast_reads_its_range_at_its_rank() {
    // 10 damage a meter of range; 1 for a global range, which reads as `()`.
    let script = r#"
fn on_resolve(ctx, caster, target) {
    if ctx.range == () {
        ctx.damage(target, "true", 1);
    } else {
        ctx.damage(target, "true", ctx.range * 10);
    }
}
"#;
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let near = Abilities::load(&mut game, "strike", &Abilities::strike(), script);
    let global = ActionData {
        keys: ActionKeys {
            range: Some(Ranked::One(RangeField::Range(ActionRange::Global))),
            ..Abilities::strike().keys
        },
        ..Abilities::strike()
    };
    let far = Abilities::load(&mut game, "far", &global, script);
    for (action, damage) in [(near, 50), (far, 1)] {
        let caster = Abilities::caster(&mut game, action, 1);
        let target = Abilities::spawn_combatant(&mut game, (3, 0, 0), Team::new(1), ());
        Abilities::cast(&mut game, caster, ActionTarget::Unit(target));
        assert_eq!(Stats::health(&game, target), 500 - damage);
    }
    assert_eq!(Abilities::failed_calls(&mut game), []);
}

#[test]
fn each_caster_draws_from_its_own_sequence() {
    // Each caster's Lash Out strikes the enemy beside it for two draws: tens, then ones.
    let script = r#"
fn on_resolve(ctx, caster, target) {
    let digits = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    for unit in ctx.find(caster, caster.position, ctx.params.radius, "enemies") {
        ctx.damage(unit, "true", ctx.pick(digits) * 10 + ctx.pick(digits));
    }
}
"#;
    let mut game = abilities_match();
    Abilities::load_stats(&mut game);
    let action = Abilities::load(&mut game, "lash_out", &lash_out(), script);
    let first = Abilities::caster(&mut game, action, 1);
    let second = Abilities::spawn_caster(
        &mut game,
        (20, 0, 0),
        Team::new(0),
        (
            Owner::new(PlayerSlot::new(0)),
            TimedSlots::new(ActionSlots::new([(action, SlotKind::new(0), Rank::new(1))])),
        ),
    );
    let targets = [Num::int(1), Num::int(21)]
        .map(|x| Abilities::spawn_combatant(&mut game, Vec3::from((x, 0, 0)), Team::new(1), ()));
    Abilities::casts(
        &mut game,
        &[(first, ActionTarget::None), (second, ActionTarget::None)],
    );
    assert_eq!(Abilities::failed_calls(&mut game), []);
    // The sequence of `script.draw` for each caster in this tick, opened as the world opens it.
    let opener = game.resource::<SimRng>().opener();
    let sequence = |caster: StableId| opener.open(RngStream::new("script.draw"), caster);
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
        let mut game = Abilities::match_of(limits, &declared);
        let data = ActionData {
            keys: ActionKeys {
                params: BTreeMap::new(),
                ..lash_out().keys
            },
            ..lash_out()
        };
        let spin = Abilities::load(
            &mut game,
            "spin",
            &data,
            "fn on_resolve(ctx, caster, target) { loop {} }",
        );
        let strike = Abilities::load(&mut game, "strike", &Abilities::strike(), STRIKE);
        let spinner = Abilities::caster(&mut game, spin, 1);
        let striker = Abilities::spawn_caster(
            &mut game,
            (0, 0, 1),
            Team::new(0),
            (
                Owner::new(PlayerSlot::new(1)),
                TimedSlots::new(ActionSlots::new([(strike, SlotKind::new(0), Rank::new(1))])),
            ),
        );
        let enemy = Abilities::spawn_combatant(&mut game, (1, 0, 0), Team::new(1), ());
        let casts = [
            (spinner, ActionTarget::None),
            (striker, ActionTarget::Unit(enemy)),
        ];
        Abilities::casts(&mut game, if spins { &casts } else { &casts[1..] });
        let limited = FailedCall {
            id: Some(spinner),
            hook: Release::hook("on_resolve"),
            kind: FailureKind::CallLimit,
        };
        let expected: Vec<_> = spins.then_some(limited).into_iter().collect();
        assert_eq!(Abilities::failed_calls(&mut game), expected);
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
fn a_channel_with_no_end_pays_by_its_level_until_an_order_waits_out_its_wind_down() {
    // Hack, as Zero Hour's hacker: a windup of 1000 ms, 30 ticks at 30 a second, spread by half
    // each way; then a channel of no end that ticks each 100 ms, 3 ticks, and pays its owner 2
    // gold at level 1 on `valor` and 5 at level 2; an order that cuts it waits 200 ms, 6 ticks.
    let declared = [
        Capability::Stats,
        Capability::Combat,
        Capability::Abilities,
        Capability::Navigation,
        Capability::Orders,
        Capability::Progression,
    ];
    let mut game = Abilities::match_of(ScriptLimits::ROOMY, &declared);
    Navigation::load_paths(&mut game, Paths::default());
    let book = TrackBook::sample();
    book.clone().load_into(&mut game);
    let cast = || ActionData::of(Targeting::None, CastData::default());
    let hack = ActionData {
        keys: ActionKeys {
            windup_ms: Some(Ranked::One(int(1000))),
            channel: Some(ChannelData {
                duration_ms: None,
                tick_ms: Ranked::One(int(100)),
                damage_interrupts: false,
                windup_spread: Some(Ranked::One(Number::Value(Scalar::Decimal(Num::HALF)))),
                wind_down_ms: Some(Ranked::One(int(200))),
                pays: Some(ChannelPaysData {
                    resource: DeclaredName::new("gold").unwrap(),
                    track: Some(DeclaredName::new("valor").unwrap()),
                    amount: vec![2, 5],
                }),
            }),
            ..cast().keys
        },
        ..cast()
    };
    let hack = Actions::load(&mut game, PackageIndex::MODE, "hack", &hack, None, 1).unwrap();
    let gold = |game: &Match| {
        let resources = game.player_resources(PlayerSlot::new(0)).unwrap();
        resources.amount(ResourceId::new(0))
    };
    let after = |game: &mut Match, tick: u64| game.run_until(tick + 1);
    let underway = |game: &Match, id| game.part::<ActionProgress>(id).in_progress();
    let order = |game: &mut Match, directive| {
        game.step_with(&[(PlayerSlot::new(0), &Order::payload(&[directive]))]);
    };
    let walk = |id, x, z| {
        let to = GroundPoint {
            x: Num::int(x),
            z: Num::int(z),
        };
        Order::one(id, OrdersDirective::Move { to })
    };
    // Cast at tick 0, its windup 30 × (1/2 + f) ticks, to whole ticks down, f the caster's draw
    // of `abilities.windup` in that tick: 30 × (1/2 + 9 234 716 / 2²⁴) = 31.51, so 31.
    let first = Abilities::walking_caster(&mut game, hack);
    Abilities::cast(&mut game, first, ActionTarget::None);
    let opener = game.resource::<SimRng>().opener();
    let fraction = opener
        .open(RngStream::new("abilities.windup"), first)
        .fraction();
    assert_eq!(fraction, Num::from_bits(9_234_716));
    let Some(InProgress::Order {
        phase: OrderPhase::Started(started),
        ..
    }) = underway(&game, first)
    else {
        panic!("Hack winds up");
    };
    assert_eq!(started.resolves_at, Tick::new(31));
    // It channels from tick 32 and pays 2 in ticks 35, 38 and 41.
    after(&mut game, 34);
    assert_eq!(gold(&game), 0);
    after(&mut game, 35);
    assert_eq!(gold(&game), 2);
    after(&mut game, 41);
    assert_eq!(gold(&game), 6);
    // A walk to z = 5 in tick 42 cuts it before its tick 44; it winds down to tick 48, where a
    // walk to x = 5, given in tick 45, replaced the first: the unit stands, paid no more, then
    // walks along x from tick 49, a meter a tick.
    order(&mut game, walk(first, 0, 5));
    let wind_down = InProgress::WindDown {
        slot: 0,
        until: Tick::new(48),
    };
    assert_eq!(underway(&game, first), Some(wind_down));
    after(&mut game, 44);
    order(&mut game, walk(first, 5, 0));
    after(&mut game, 48);
    assert_eq!(underway(&game, first), None);
    assert_eq!(game.part::<Position>(first).get(), Vec3::ZERO);
    assert_eq!(gold(&game), 6);
    after(&mut game, 50);
    assert_eq!(game.part::<Position>(first).get(), Vec3::from((2, 0, 0)));

    // At level 2 on `valor`, 50 into it, a caster pays 5 a tick. A stop in its wind-down ends it
    // at once and drops the walk it holds, so the unit stands; a death in its channel ends it at
    // once too, with no wind-down: neither pays again.
    let channel = |game: &mut Match, x: i16| {
        let id = Abilities::walking_caster(game, hack);
        *game.part_mut::<Position>(id) = Position::from((x, 0, 0));
        let mut experience = Experience::new(TrackSet::from([TrackId::new(1)]));
        experience.add(TrackId::new(1), Num::int(50), &book);
        game.insert_parts(id, experience);
        Abilities::cast(game, id, ActionTarget::None);
        let Some(InProgress::Order {
            phase: OrderPhase::Started(started),
            ..
        }) = underway(game, id)
        else {
            panic!("Hack winds up");
        };
        let paid = started.resolves_at.get() + 4;
        let held = gold(game);
        after(game, paid);
        assert_eq!(gold(game), held + 5);
        id
    };
    let stopped = channel(&mut game, 20);
    order(&mut game, walk(stopped, 20, 5));
    assert!(matches!(
        underway(&game, stopped),
        Some(InProgress::WindDown { .. })
    ));
    order(&mut game, Order::one(stopped, OrdersDirective::Stop));
    assert_eq!(underway(&game, stopped), None);
    assert_eq!(game.part::<HeldOrder>(stopped).get(), None);
    let killed = channel(&mut game, 40);
    Combat::queue_damage(&mut game, None, killed, Num::int(1000), "true");
    game.step();
    assert_eq!(underway(&game, killed), None);
    let held = gold(&game);
    let later = game.now().get() + 9;
    game.run_until(later);
    assert_eq!(gold(&game), held);
    assert_eq!(game.part::<Position>(stopped).get(), Vec3::from((20, 0, 0)));
    assert_eq!(Abilities::failed_calls(&mut game), []);
}
