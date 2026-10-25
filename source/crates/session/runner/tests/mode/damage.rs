use std::num::NonZeroU64;

use campfire_abilities::Abilities;
use campfire_actions::{ActionProgress, SlotTimes};
use campfire_combat::Combat;

use campfire_common::Tick;
use campfire_script::CapabilityRefusal;
use campfire_sim::internals::SimWorldInternals;
use campfire_stats::Applier;
use campfire_values::Allegiance;

use super::*;

/// The allegiance of the attacks of the source, of team 0 and no player.
const SOURCE: Allegiance = Allegiance {
    team: Team::new(0),
    owner: None,
};

#[test]
fn the_3v3s_calc_damage_weighs_each_hit_exactly() {
    // The source deals 50% more, crits on a roll below 0.25, ignores half of armor, then 10
    // more.
    let mut set = mode_packages(CALC_DAMAGE_3V3);
    let source = [
        ("damage_dealt_pct", "\"0.5\""),
        ("crit_chance", "\"0.25\""),
        ("armor_pen_pct", "\"0.5\""),
        ("armor_pen", "10"),
    ];
    let armored = [
        ("armor", "120"),
        ("magic_resist", "60"),
        ("physical_block", "5"),
    ];
    for (name, stats) in [
        ("source", &source[..]),
        ("armored", &armored[..]),
        ("exposed", &[("armor", "-100")][..]),
    ] {
        set.edit(UNITS, Edit::Append(&stat_type(name, stats)));
    }
    let mut game = started(&set, ScriptLimits::ROOMY).unwrap();
    let source = spawn_of(&mut game, "source", Team::new(0));
    let armored = spawn_of(&mut game, "armored", Team::new(1));
    let exposed = spawn_of(&mut game, "exposed", Team::new(1));
    Mode::tick_inputs(&mut game, &[]);
    let crit = DamageCause::Attack {
        roll: Num::from_bits((1 << Num::FRAC_BITS) / 4 - 1),
        rank: Rank::FIRST,
        allegiance: SOURCE,
    };
    let attack = DamageCause::Attack {
        roll: Num::ONE / 4,
        rank: Rank::FIRST,
        allegiance: SOURCE,
    };
    // 100 physical, its roll of 0.25 no crit, 150 dealt: armor 120 × (1 − 0.5) − 10 = 50, 150 ×
    // 100 ÷ 150 = 100, less the block of 5: 95.
    Mode::damage_of_kind(&mut game, Some(source), armored, 100, "physical", attack);
    // 40 magic, a crit on a roll just below 0.25: 40 × 1.5 × 2 = 120, magic resist 60 with no
    // magic pen: 120 × 100 ÷ 160 = 75, no block.
    Mode::damage_of_kind(&mut game, Some(source), armored, 40, "magic", crit);
    // 100 physical, 150 dealt, against armor −100, which pen does not touch: 150 × (2 − 100 ÷
    // 200) = 225.
    Mode::damage_of_kind(&mut game, Some(source), exposed, 100, "physical", attack);
    // 100 true, 150 dealt, whatever the armor.
    Mode::damage_of_kind(
        &mut game,
        Some(source),
        exposed,
        100,
        "true",
        DamageCause::Effect,
    );
    // An attack of 100 physical from no source, its roll 0: no bonus and no crit chance, 100 ×
    // 1.5 = 150.
    let lowest = DamageCause::Attack {
        roll: Num::ZERO,
        rank: Rank::FIRST,
        allegiance: SOURCE,
    };
    Mode::damage_of_kind(&mut game, None, exposed, 100, "physical", lowest);
    Mode::tick_inputs(&mut game, &[]);
    assert!(Mode::failures(&mut game).is_empty());
    assert_eq!(
        game.part::<Life>(armored).current(),
        Num::int(1000 - 95 - 75)
    );
    assert_eq!(
        game.part::<Life>(exposed).current(),
        Num::int(1000 - 225 - 150 - 150)
    );
}

#[test]
fn armor_takes_its_share_of_a_kind_before_calc_damage_weighs_it() {
    // `calc_damage` adds 1 to what reaches it. A grunt of no armor takes 10 physical whole: 11. A
    // plate takes physical at 0.5 and names no magic: 10 physical is 5 before the mode weighs it,
    // 6, where a share after the mode's 11 would give 5.5; 10 magic stays 11. While it carries
    // `plating`, whose tag `plated` its `armors` test, it takes physical at 2: 21, and magic
    // whole, as `double` names none; once `plating` ends, 6 again.
    let mut set = mode_packages("fn calc_damage(ctx, d) { d.amount + 1 }");
    set.edit(
        DATA,
        Edit::Append(
            "\n[combat.armors.half]\nphysical = \"0.5\"\n\n[combat.armors.double]\n\
             physical = \"2\"\n\n[modifiers.plating]\ntags = [\"plated\"]\n",
        ),
    );
    let plate = replaced(
        &stat_type("plate", &[]),
        "on_death = \"stay\" }",
        "on_death = \"stay\", armor = \"half\", \
         armors = [{ when = \"all:plated\", armor = \"double\" }] }",
    );
    set.edit(
        UNITS,
        Edit::Append(&format!("{}{plate}", stat_type("dummy", &[]))),
    );
    let mut game = started(&set, ScriptLimits::ROOMY).unwrap();
    let [grunt, plate] = ["dummy", "plate"].map(|name| spawn_of(&mut game, name, Team::new(1)));
    let plating = Stats::modifier(&game, PackageIndex::MODE, "plating").unwrap();
    Mode::tick_inputs(&mut game, &[]);
    // What `unit` takes of 10 of `kind`.
    let taken = |game: &mut Match, unit, kind| {
        let life = |game: &Match| game.part::<Life>(unit).current();
        let before = life(game);
        Mode::damage_of_kind(game, None, unit, 10, kind, DamageCause::Effect);
        Mode::tick_inputs(game, &[]);
        assert!(Mode::failures(game).is_empty());
        before - life(game)
    };
    assert_eq!(taken(&mut game, grunt, "physical"), Num::int(11));
    assert_eq!(taken(&mut game, plate, "physical"), Num::int(6));
    assert_eq!(taken(&mut game, plate, "magic"), Num::int(11));
    Stats::give_modifier(&mut game, plate, plating, Applier::MODE);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(taken(&mut game, plate, "physical"), Num::int(21));
    assert_eq!(taken(&mut game, plate, "magic"), Num::int(11));
    assert_eq!(taken(&mut game, grunt, "physical"), Num::int(11));
    Stats::take_modifier(&mut game, plate, plating, None);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(taken(&mut game, plate, "physical"), Num::int(6));
}

#[test]
fn a_weapon_of_when_fires_while_its_holders_tags_pass_it_and_its_slots_stay() {
    // A gunner's spell slots, learned at its spawn, hold `focus`, an ability of a cooldown of
    // 1000 ms, 10 ticks at 10 a second, and of the passive `steady`, that it uses and holds while
    // it is `plated`; `heavy`, 25 true damage while it is `plated`; and `light`, 10 while it is
    // not; each weapon of no windup and a period of 10 ticks, each slot ready on its own.
    let mut set = mode_packages("");
    let weapon = |name: &str, damage: u8, when: &str| {
        format!(
            "\n[actions.{name}]\nkind = \"attack\"\ntargeting = \"enemies\"\nrange = \"5\"\n\
             period_ms = 1000\namount = {damage}\ndamage_kind = \"true\"\nwhen = \"{when}\"\n"
        )
    };
    let data = format!(
        "\n[modifiers.plating]\ntags = [\"plated\"]\n\n[actions.focus]\ntargeting = \"none\"\n\
         cooldown_ms = 1000\nwhen = \"all:plated\"\npassive_modifier = \"steady\"\n\n\
         [modifiers.steady]\n{}{}",
        weapon("heavy", 25, "all:plated"),
        weapon("light", 10, "all:!plated"),
    );
    set.edit(DATA, Edit::Append(&data));
    let gunner =
        stat_type("gunner", &[]) + "slots = { spell = [\"focus\", \"heavy\", \"light\"] }\n";
    set.edit(
        UNITS,
        Edit::Append(&(gunner + stat_type("dummy", &[]).as_str())),
    );
    let mut game = started(&set, ScriptLimits::ROOMY).unwrap();
    let gunner = spawn_of(&mut game, "gunner", Team::new(0));
    let dummy = spawn_of(&mut game, "dummy", Team::new(1));
    let [plating, steady] = ["plating", "steady"]
        .map(|name| Stats::modifier(&game, PackageIndex::MODE, name).expect("the mode's modifier"));
    let steadied = |game: &Match| {
        let carried = Stats::carried_modifiers(game, gunner);
        carried.iter().any(|&(modifier, _)| modifier == steady)
    };
    let slots = game.part::<ActionSlots>(gunner).clone();
    // Each of the next `ticks` ticks in which the dummy loses life, from 0, and what it loses.
    let drops = |game: &mut Match, ticks| {
        let mut drops = Vec::new();
        for tick in 0..ticks {
            let before = game.part::<Life>(dummy).current();
            Mode::tick_inputs(game, &[]);
            let lost = before - game.part::<Life>(dummy).current();
            if lost != Num::ZERO {
                drops.push((tick, lost));
            }
        }
        assert!(Mode::failures(game).is_empty());
        drops
    };
    let [light, heavy] = [10, 25].map(Num::int);
    let ready = |game: &Match| game.part::<SlotTimes>(gunner).slot(0).unwrap().ready_at;
    let ordered = |game: &Match| game.part::<ActionProgress>(gunner).has_order();
    // Unplated, an order of `focus` does not start, as an order its tags block waits: it stays
    // ordered, its slot ready at 0, 5 ticks on.
    Abilities::cast(&mut game, gunner, ActionTarget::None);
    assert_eq!(drops(&mut game, 5), []);
    assert_eq!((ordered(&game), ready(&game)), (true, Tick::new(0)));
    assert!(!steadied(&game));
    // Plated from the next tick: `focus` starts in it, ready 10 ticks on, and `steady` holds.
    Stats::give_modifier(&mut game, gunner, plating, Applier::MODE);
    let now = game.now();
    assert_eq!(drops(&mut game, 1), []);
    assert_eq!(
        (ordered(&game), ready(&game)),
        (false, Tick::new(now.get() + 10))
    );
    assert!(steadied(&game));
    // The attack: `heavy` strikes in the tick of the order and 10 ticks on, `light`, ready, not.
    Combat::attack(&mut game, gunner, dummy);
    assert_eq!(drops(&mut game, 20), [(0, heavy), (10, heavy)]);
    assert_eq!(game.part::<ActionSlots>(gunner), &slots);
    // Unplated from the next tick: `light`, never fired, strikes in it and 10 on, and `steady`
    // ends.
    Stats::take_modifier(&mut game, gunner, plating, None);
    assert_eq!(drops(&mut game, 20), [(0, light), (10, light)]);
    assert!(!steadied(&game));
    assert_eq!(game.part::<ActionSlots>(gunner), &slots);
}

#[test]
fn calc_damage_and_calc_heal_are_pure_outside_the_pools_and_a_failure_keeps_the_amount() {
    let script = r#"
fn calc_damage(ctx, d) {
    if d.kind == "magic" {
        ctx.after("late", 100);
    }
    if d.kind == "true" {
        return "none";
    }
    d.amount * 3
}

fn calc_heal(ctx, h) {
    if h.is_leech { h.amount } else { h.amount / 2 }
}
"#;
    // A mode pool of 30 operations, a call's most, which the five calls of `calc_damage` and
    // `calc_heal` below would spend were they drawn from it.
    let limits = ScriptLimits {
        per_call: NonZeroU64::new(30).unwrap(),
        mode: 30,
        ..ScriptLimits::ROOMY
    };
    let mut set = mode_packages(script);
    set.edit(UNITS, Edit::Append(&stat_type("dummy", &[])));
    let mut game = started(&set, limits).unwrap();
    let source = spawn_of(&mut game, "dummy", Team::new(0));
    let target = spawn_of(&mut game, "dummy", Team::new(1));
    Mode::tick_inputs(&mut game, &[]);
    // Physical 10, tripled; magic 10, whose timer the pure `ctx` refuses; true 10, which returns
    // no number. The two failures keep their 10: 1000 − 30 − 10 − 10.
    for kind in DAMAGE_KINDS {
        Mode::damage_of_kind(
            &mut game,
            Some(source),
            target,
            10,
            kind,
            DamageCause::Effect,
        );
    }
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(
        Mode::failures(&mut game),
        [
            FailureKind::Api(ApiError::PureCall),
            FailureKind::Api(ModeRefusal::NotAnAmount.refused())
        ]
    );
    assert_eq!(game.part::<Life>(target).current(), Num::int(950));
    let timers = game.resource::<Timers>();
    assert!(timers.due(Tick::new(u64::MAX)).is_none());

    // A heal of 10 that `calc_heal` halves, and a leech heal of 6 it keeps: 950 + 5 + 6, as
    // the target has no heal scale.
    for (amount, cause) in [(10, HealCause::Effect), (6, HealCause::Leech)] {
        DamagePass::queue(
            &mut game,
            PassEntry::Heal(Heal {
                source: Some(source),
                target,
                amount: Num::int(amount),
                cause,
                action: None,
                depth: 0,
            }),
        );
    }
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(Mode::failures(&mut game), []);
    assert_eq!(game.part::<Life>(target).current(), Num::int(961));
}
