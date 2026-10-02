use super::*;

#[test]
fn a_mode_learns_a_hero_ability_up_to_its_last_rank_and_a_failed_call_learns_nothing() {
    // Hero X holds its ability in slot 0, of 2 ranks, and the spell in slot 1, of 1.
    let learner = r#"
fn on_mode_input(ctx, player, name, value) {
    if name == "spells" {
        ctx.choose(player, "spells", value);
        return;
    }
    if name == "hero" {
        pick(ctx, player, value);
        return;
    }
    let hero = ctx.avatars()[0];
    let slot = if value == "spell" { 1 } else if value == "none" { 2 } else if value == "negative" { -1 } else { 0 };
    let times = if value == "twice" { 2 } else if value == "thrice" { 3 } else { 1 };
    for time in 0..times {
        ctx.learn(hero, slot);
    }
}
"#;
    let mut game = Game::new(learner, ScriptLimits::ROOMY);
    let spells = ModeInput {
        name: "spells",
        value: InputValue::StringList(vec!["blink"]),
    };
    game.tick(&[(0, spells), (0, input("hero", "hero-x"))]);
    let mut owned = game.sim.world.query_filtered::<Entity, With<Owner>>();
    let hero = owned.single(&game.sim.world).unwrap();
    let ranks = |game: &Game| {
        let slots = game.sim.world.get::<ActionSlots>(hero).unwrap();
        slots.iter().map(|slot| slot.rank).collect::<Vec<_>>()
    };
    // Three ranks of two fail at the third, and the call learns none; two in one call count the
    // first queued, and reach the last rank; then neither slot has a rank more, and two slots do
    // not exist.
    let steps = [
        ("thrice", [0, 1], Some(ApiError::MaxRank)),
        ("twice", [2, 1], None),
        ("once", [2, 1], Some(ApiError::MaxRank)),
        ("spell", [2, 1], Some(ApiError::MaxRank)),
        ("none", [2, 1], Some(ApiError::NoAbilitySlot)),
        ("negative", [2, 1], Some(ApiError::NoAbilitySlot)),
    ];
    for (value, expected, failure) in steps {
        game.tick(&[(0, input("probe", value))]);
        assert_eq!(ranks(&game), expected, "{value}");
        let failures: Vec<_> = failure.into_iter().map(Some).collect();
        assert_eq!(game.failures(), failures, "{value}");
    }
}

#[test]
fn experience_raises_levels_and_each_level_reached_runs_on_level_up_in_the_tick() {
    let leveler = r#"
fn on_input(ctx, player, name, value) {
    let hero = ctx.avatars()[0];
    if value == "99" {
        ctx.add_xp(hero, "level", 99);
    } else if value == "1.5" {
        ctx.add_xp(hero, "level", num(3) / 2);
    } else if value == "500" {
        ctx.add_xp(hero, "level", 500);
    } else if value == "tower" {
        ctx.add_xp(ctx.units_tagged("tower")[0], "level", 1);
    } else if value == "fame" {
        ctx.add_xp(hero, "fame", 1);
    } else if value == "negative" {
        ctx.add_xp(hero, "level", -1);
    }
}

fn on_level_up(ctx, unit, track, level) {
    ctx.state.kind += `${track} ${level};`;
    if track == "level" && level == 3 {
        ctx.add_xp(unit, "valor", 50);
    }
}
"#;
    let mut game = Game::picking(leveler, ScriptLimits::ROOMY);
    let hero = game.pick(0, "hero-x");
    // The `level` track's level is the unit's own; valor keeps its own.
    let progress = |game: &Game| {
        let experience = game.sim.world.get::<Experience>(hero).unwrap();
        let [level, valor] = [0, 1].map(|at| experience.get(TrackId::new(at).unwrap()).unwrap());
        assert_eq!(level.level, None);
        let unit_level = game.sim.world.get::<Level>(hero).unwrap().get();
        (level.xp, unit_level, valor.xp, valor.level.map(Level::get))
    };

    let half = Num::from_bits(1 << 23);
    // 99 stays below level 2's 100. 1.5 more makes 100.5: level 2, and the unit's level with it.
    // 500 more makes 600.5, past level 3's 300, the last; level 3 adds 50 valor, valor's level 2,
    // and its `on_level_up` runs in the same tick.
    let steps = [
        ("99", (Num::int(99), 1, Num::ZERO, Some(1)), ""),
        (
            "1.5",
            (Num::int(100) + half, 2, Num::ZERO, Some(1)),
            "level 2;",
        ),
        (
            "500",
            (Num::int(600) + half, 3, Num::int(50), Some(2)),
            "level 2;level 3;valor 2;",
        ),
    ];
    for (value, expected, reached) in steps {
        game.tick(&[(0, input("probe", value))]);
        assert_eq!(progress(&game), expected, "{value}");
        assert_eq!(
            game.field("kind"),
            StateValue::Text(reached.into()),
            "{value}"
        );
        assert_eq!(game.failures(), [], "{value}");
    }
    // A unit without the track, a track the mode does not declare, and a negative amount fail
    // the call, and change nothing.
    let refused = [
        ("tower", ApiError::NoTrack),
        ("fame", ApiError::UnknownTrack),
        ("negative", ApiError::NegativeXp),
    ];
    for (value, error) in refused {
        game.tick(&[(0, input("probe", value))]);
        assert_eq!(game.failures(), [Some(error)], "{value}");
        assert_eq!(
            progress(&game),
            (Num::int(600) + half, 3, Num::int(50), Some(2)),
            "{value}"
        );
    }
}

#[test]
fn a_hero_dead_beside_an_enemy_hero_gives_it_experience_and_comes_back() {
    // The 3v3's `on_unit_died` as it is: hero Y, 10 m from hero X, dies to the mode's damage,
    // with no killer. X takes all of Y's 150 + 25 × 1 experience, past level 2's 100, and Y
    // comes back after 1000 + 500 × 1 ms, as the call that gave the experience did not fail.
    let killer = r#"
fn on_input(ctx, player, name, value) {
    ctx.damage(ctx.avatars("b")[0], 1000, "physical");
}
"#;
    let script = format!("{killer}{DEATHS_3V3}");
    let mut game = Game::picking(&script, ScriptLimits::ROOMY);
    // The 3v3's tag of its cores, which `on_unit_died` reads first.
    let core = UnitTypeData::tagged(&["core"]);
    Units::load_type(&mut game.sim.world, TypeScope::Mode, "core", &core);
    game.tick(&[(0, input("hero", "hero-x")), (2, input("hero", "hero-y"))]);
    let hero = |game: &mut Game, slot| {
        let mut owned = game.sim.world.query::<(Entity, &Owner)>();
        let (entity, _) = owned
            .iter(&game.sim.world)
            .find(|(_, owner)| owner.slot() == PlayerSlot::new(slot))
            .unwrap();
        entity
    };
    let (x, y) = (hero(&mut game, 0), hero(&mut game, 2));
    game.tick(&[(0, input("probe", "kill"))]);
    assert_eq!(game.failures(), []);
    assert!(game.sim.world.get::<Dead>(y).is_some());
    let xp = |game: &Game| {
        let experience = game.sim.world.get::<Experience>(x).unwrap();
        experience.get(TrackId::new(0).unwrap()).unwrap().xp
    };
    assert_eq!(xp(&game), Num::int(175));
    assert_eq!(game.sim.world.get::<Level>(x).unwrap().get(), 2);
    // Y died in tick 1, and the mode set its respawn from the end of tick 1, the start of tick
    // 2: 1500 ms is 15 ticks at 10 a second, so Y is dead through tick 16 and back in tick 17.
    assert_eq!(
        game.sim.world.get::<Respawn>(y),
        Some(&Respawn { at: Tick::new(17) })
    );
    for _ in 2..=16 {
        game.tick(&[]);
        assert!(game.sim.world.get::<Dead>(y).is_some());
    }
    game.tick(&[]);
    assert!(game.sim.world.get::<Dead>(y).is_none());
}
