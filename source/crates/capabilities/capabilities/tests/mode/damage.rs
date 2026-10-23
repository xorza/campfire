use campfire_common::Tick;
use campfire_script::CapabilityRefusal;
use campfire_sim::internals::SimWorldInternals;
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
        set.units.push_str(&stat_type(name, stats));
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
    assert!(Mode::failures(&game).is_empty());
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
    // `calc_damage` adds 1 to what reaches it. A grunt of no armor takes 10 physical whole: 11.
    // Its type then takes physical at 0.5 and names no magic: 10 physical is 5 before the mode
    // weighs it, 6, where a share after the mode's 11 would give 5.5; 10 magic stays 11.
    // What a grunt takes of 10 of `kind`, its type of the mode's armor `half` or of none.
    let taken = |armored: bool, kind: &str| {
        let mut set = mode_packages("fn calc_damage(ctx, d) { d.amount + 1 }");
        set.data
            .push_str("\n[combat.armors.half]\nphysical = \"0.5\"\n");
        let mut dummy = stat_type("dummy", &[]);
        if armored {
            let kept = "on_death = \"stay\" }";
            dummy = replaced(&dummy, kept, "on_death = \"stay\", armor = \"half\" }");
        }
        set.units.push_str(&dummy);
        let mut game = started(&set, ScriptLimits::ROOMY).unwrap();
        let grunt = spawn_of(&mut game, "dummy", Team::new(1));
        Mode::tick_inputs(&mut game, &[]);
        let life = |game: &Match| game.part::<Life>(grunt).current();
        let before = life(&game);
        Mode::damage_of_kind(&mut game, None, grunt, 10, kind, DamageCause::Effect);
        Mode::tick_inputs(&mut game, &[]);
        assert!(Mode::failures(&game).is_empty());
        before - life(&game)
    };
    assert_eq!(taken(false, "physical"), Num::int(11));
    assert_eq!(taken(true, "physical"), Num::int(6));
    assert_eq!(taken(true, "magic"), Num::int(11));
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
    // A mode pool of one operation, which no call of `calc_damage` draws from.
    let limits = ScriptLimits {
        mode: 1,
        ..ScriptLimits::ROOMY
    };
    let mut set = mode_packages(script);
    set.units.push_str(&stat_type("dummy", &[]));
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
        Mode::failures(&game),
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
    assert_eq!(Mode::failures(&game), []);
    assert_eq!(game.part::<Life>(target).current(), Num::int(961));
}
