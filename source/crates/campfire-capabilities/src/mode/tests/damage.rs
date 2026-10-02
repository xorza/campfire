use super::*;

#[test]
fn the_3v3s_calc_damage_weighs_each_hit_exactly() {
    let mut game = Game::new(CALC_DAMAGE_3V3, ScriptLimits::ROOMY);
    let half = Num::HALF;
    // The source deals 50% more, crits on a roll below 0.25, ignores half of armor, then 10
    // more.
    let source = game.fighter(
        0,
        &[
            ("damage_dealt_pct", half),
            ("crit_chance", Num::ONE / 4),
            ("armor_pen_pct", half),
            ("armor_pen", Num::int(10)),
        ],
    );
    let armored = game.fighter(
        1,
        &[
            ("armor", Num::int(120)),
            ("magic_resist", Num::int(60)),
            ("physical_block", Num::int(5)),
        ],
    );
    let exposed = game.fighter(1, &[("armor", Num::int(-100))]);
    game.tick(&[]);
    let crit = DamageCause::Attack {
        roll: Num::from_bits((1 << Num::FRAC_BITS) / 4 - 1),
    };
    let attack = DamageCause::Attack { roll: Num::ONE / 4 };
    // 100 physical, its roll of 0.25 no crit, 150 dealt: armor 120 × (1 − 0.5) − 10 = 50, 150 ×
    // 100 ÷ 150 = 100, less the block of 5: 95.
    game.damage(Some(source), armored, 100, "physical", attack);
    // 40 magic, a crit on a roll just below 0.25: 40 × 1.5 × 2 = 120, magic resist 60 with no
    // magic pen: 120 × 100 ÷ 160 = 75, no block.
    game.damage(Some(source), armored, 40, "magic", crit);
    // 100 physical, 150 dealt, against armor −100, which pen does not touch: 150 × (2 − 100 ÷
    // 200) = 225.
    game.damage(Some(source), exposed, 100, "physical", attack);
    // 100 true, 150 dealt, whatever the armor.
    game.damage(Some(source), exposed, 100, "true", DamageCause::Effect);
    // An attack of 100 physical from no source, its roll 0: no bonus and no crit chance, 100 ×
    // 1.5 = 150.
    let lowest = DamageCause::Attack { roll: Num::ZERO };
    game.damage(None, exposed, 100, "physical", lowest);
    game.tick(&[]);
    assert!(game.failures().is_empty());
    assert_eq!(game.sim.life(armored), Num::int(1000 - 95 - 75));
    assert_eq!(game.sim.life(exposed), Num::int(1000 - 225 - 150 - 150));
}

#[test]
fn calc_damage_and_calc_heal_are_pure_outside_the_pools_and_a_failure_keeps_the_amount() {
    let script = r#"
fn calc_damage(ctx, d) {
    if d.kind == "magic" {
        ctx.timer("late", 100, false, ());
    }
    if d.kind == "true" {
        return "none";
    }
    d.amount * 3
}

fn calc_heal(ctx, h) {
    if h.leech { h.amount } else { h.amount / 2 }
}
"#;
    // A mode pool of one operation, which no call of `calc_damage` draws from.
    let limits = ScriptLimits {
        mode: 1,
        ..ScriptLimits::ROOMY
    };
    let mut game = Game::new(script, limits);
    let source = game.fighter(0, &[]);
    let target = game.fighter(1, &[]);
    game.tick(&[]);
    // Physical 10, tripled; magic 10, whose timer the pure `ctx` refuses; true 10, which returns
    // no number. The two failures keep their 10: 1000 − 30 − 10 − 10.
    for kind in DAMAGE_KINDS {
        game.damage(Some(source), target, 10, kind, DamageCause::Effect);
    }
    game.tick(&[]);
    assert_eq!(
        game.failures(),
        [Some(ApiError::PureCall), Some(ApiError::NotAnAmount)]
    );
    assert_eq!(game.sim.life(target), Num::int(950));
    let timers = game.sim.world.resource::<Timers>();
    assert!(timers.due(Tick::new(u64::MAX)).is_none());

    // A heal of 10 that `calc_heal` halves, and a leech heal of 6 it keeps: 950 + 5 + 6, as
    // the target has no heal scale.
    for (amount, cause) in [(10, HealCause::Effect), (6, HealCause::Leech)] {
        game.sim.world.resource_mut::<PassQueue>().push_heal(Heal {
            source: Some(source),
            target,
            amount: Num::int(amount),
            cause,
            ability: None,
            depth: 0,
        });
    }
    game.tick(&[]);
    assert_eq!(game.failures(), []);
    assert_eq!(game.sim.life(target), Num::int(961));
}
