use super::*;

#[test]
fn a_player_spends_only_their_own_pool() {
    // Each player's pool holds one whole call of 1000: player 0's spin runs its 1000 and fails,
    // which spends its pool, so its next input fails unrun; player 1's input in the same tick
    // runs from its own pool.
    let spin = r#"
fn on_mode_input(ctx, player, name, value) {
    ctx.state.inputs += 1;
    if name == "fail" {
        loop {}
    }
}
"#;
    let limits = ScriptLimits {
        per_call: 1000,
        player: 1000,
        ..ScriptLimits::ROOMY
    };
    let mut game = Game::new(spin, limits);
    let inputs = |game: &Game| game.field("inputs");
    game.tick(&[
        (0, input("fail", "")),
        (0, input("phase", "")),
        (1, input("phase", "")),
    ]);
    let failures = game.sim.world.non_send::<ScriptFailures>();
    let errors: Vec<_> = failures
        .get()
        .iter()
        .map(|failure| &failure.error)
        .collect();
    assert!(
        matches!(
            errors[..],
            [
                CallError::Script(ScriptError::CallLimit),
                CallError::Script(ScriptError::TickBudget)
            ]
        ),
        "{errors:?}"
    );
    assert_eq!(inputs(&game), StateValue::Int(1));
    // Every pool starts full in the next tick.
    game.tick(&[(0, input("phase", ""))]);
    assert_eq!(inputs(&game), StateValue::Int(2));
}

#[test]
fn a_timer_whose_call_finds_the_mode_pool_spent_stays_due() {
    // A pool of 1500 operations: the first spinning call runs its 1000 and fails, which leaves
    // 500; the second ends past those, and its timer waits for the next tick.
    let spin = r#"
fn on_match_start(ctx) {
    ctx.timer("a", 100, false, ());
    ctx.timer("b", 100, false, ());
}

fn on_timer(ctx, name, data) {
    ctx.state.count += 1;
    loop {}
}
"#;
    let limits = ScriptLimits {
        per_call: 1000,
        mode: 1500,
        ..ScriptLimits::ROOMY
    };
    let mut game = Game::new(spin, limits);
    let due = |game: &Game| {
        let timers = game.sim.world.resource::<Timers>();
        (
            timers
                .due(Tick::new(u64::MAX))
                .map(|timer| timer.name.clone()),
            timers.due(Tick::ZERO).is_some(),
        )
    };
    game.tick(&[]);
    assert_eq!(due(&game), (Some("b".to_owned()), false));
    game.tick(&[]);
    assert_eq!(due(&game), (None, false));
    // Both calls failed, so none counted.
    assert_eq!(game.field("count"), StateValue::Int(0));
}

#[test]
fn a_death_or_a_level_up_whose_call_finds_the_mode_pool_spent_waits_with_its_unit() {
    // Each death call spins 50 additions: a mode pool of 500 operations holds two of them, not
    // three, and then the third and a level-up.
    let script = r#"
fn on_match_start(ctx) {
    ctx.spawn_group("b", "mid", "end", ["grunt", "grunt", "grunt"]);
}

fn on_mode_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
    } else {
        ctx.add_xp(ctx.avatars()[0], "level", 100);
    }
}

fn on_unit_died(ctx, unit, killer, assisters) {
    ctx.state.kind += `${unit.team} by ${killer.team};`;
    let spun = 0;
    for i in 0..50 { spun += i; }
}

fn on_level_up(ctx, unit, track, level) {
    ctx.state.kind += `${track} ${level};`;
}
"#;
    let limits = ScriptLimits {
        mode: 500,
        ..ScriptLimits::ROOMY
    };
    let mut game = Game::new(script, limits);
    game.tick(&[(0, input("hero", "hero-x"))]);
    // Units: the tower 0 of a, b's grunts 1 to 3, which despawn when they die, and the hero 4.
    // The tower's damage kills the three grunts in tick 1, as the hero reaches level 2.
    let grunts = [1, 2, 3].map(|at| game.entity(at));
    let tower = game.sim.world.get::<StableId>(game.entity(0)).copied();
    for grunt in grunts {
        game.sim.world.entity_mut(grunt).insert(OnDeath::Despawn);
        let target = *game.sim.world.get::<StableId>(grunt).unwrap();
        game.sim
            .world
            .resource_mut::<PassQueue>()
            .push_damage(Damage {
                source: tower,
                target,
                amount: Num::int(10),
                kind: DamageKind::new(0),
                cause: DamageCause::Effect,
                ability: None,
                depth: 0,
                hit: None,
            });
    }
    let waiting = |game: &Game| {
        let deaths = game.sim.world.resource::<UnansweredDeaths>().iter().count();
        (deaths, game.sim.world.resource::<LevelUps>().0.len())
    };
    let stands = |game: &Game, grunt: Entity| {
        let unit = game.sim.world.get_entity(grunt).ok()?;
        Some((unit.contains::<Dead>(), unit.contains::<Kept>()))
    };
    game.tick(&[(0, input("probe", "xp"))]);
    // Two deaths ran; the third waits, and so does the level-up behind it. The first two grunts
    // despawned; the third stays, dead and kept, for its call.
    assert_eq!(
        game.field("kind"),
        StateValue::Text("b by a;b by a;".to_owned())
    );
    assert_eq!(waiting(&game), (1, 1));
    // The waiting death is state: it decodes to itself, and an assister its runs do not cover
    // fails to decode. Its assisters come last, none: their length is the last byte.
    let unanswered = game.sim.world.resource::<UnansweredDeaths>();
    let mut bytes = postcard::to_allocvec(unanswered).unwrap();
    let decoded = postcard::from_bytes::<UnansweredDeaths>(&bytes).ok();
    assert_eq!(decoded.as_ref(), Some(unanswered));
    assert_eq!(bytes.pop(), Some(0));
    bytes.extend([1, 5]);
    assert!(postcard::from_bytes::<UnansweredDeaths>(&bytes).is_err());
    assert_eq!(
        grunts.map(|grunt| stands(&game, grunt)),
        [None, None, Some((true, true))]
    );
    game.tick(&[]);
    // Both run first in the next tick, with the third grunt and its killer as handles; then it
    // despawns.
    let all = "b by a;b by a;b by a;level 2;";
    assert_eq!(game.field("kind"), StateValue::Text(all.to_owned()));
    assert_eq!(waiting(&game), (0, 0));
    assert_eq!(stands(&game, grunts[2]), None);
    assert_eq!(game.failures(), []);
}
