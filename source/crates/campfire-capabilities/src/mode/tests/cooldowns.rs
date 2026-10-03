use super::*;

#[test]
fn a_script_cuts_cooldowns_by_time_and_by_share_down_to_now() {
    let cutter = r#"
fn on_input(ctx, player, name, value) {
    let x = ctx.avatars("a")[0];
    if value == "cut" {
        ctx.reduce_cooldown(x, "strike", 1000);
    } else if value == "half" {
        ctx.reduce_cooldowns(x, "basic", num(1) / 2);
    } else if value == "all" {
        ctx.reduce_cooldown(x, "strike", 100000);
    } else if value == "unknown" {
        ctx.reduce_cooldown(x, "lunge", 1000);
    } else if value == "unheld" {
        ctx.reduce_cooldown(ctx.avatars("b")[0], "strike", 1000);
    } else if value == "negative" {
        ctx.reduce_cooldown(x, "strike", -1);
    } else if value == "kind" {
        ctx.reduce_cooldowns(x, "ultimate", 1);
    } else if value == "share" {
        ctx.reduce_cooldowns(x, "basic", 2);
    }
}
"#;
    let mut game = Game::picking(cutter, ScriptLimits::ROOMY);
    // Hero X holds `strike` in its basic slot 0; hero Y does not hold it.
    let x = game.pick(0, "hero-x");
    game.pick(2, "hero-y");
    let ready = |game: &Game| {
        let slots = game.sim.world.get::<ActionSlots>(x).unwrap();
        slots.slot(0).unwrap().ready_at.get()
    };
    // Ready 50 ticks from the call's tick; 1000 ms is 10 ticks at 10 a second: 40 from then.
    let start = game.sim.now().get();
    game.sim
        .world
        .get_mut::<ActionSlots>(x)
        .unwrap()
        .cool_down(0, Tick::new(start + 50));
    game.tick(&[(0, input("probe", "cut"))]);
    assert_eq!(ready(&game), start + 40);
    // A tick later 39 are left; half of them is 19.5, which rounds up to 20 kept.
    game.tick(&[(0, input("probe", "half"))]);
    assert_eq!(ready(&game), start + 1 + 20);
    // A cut past what is left makes it ready in the call's tick, no earlier.
    game.tick(&[(0, input("probe", "all"))]);
    assert_eq!(ready(&game), start + 2);
    assert_eq!(game.failures(), []);
    let refused = [
        ("unknown", ApiError::UnknownAbility),
        ("unheld", ApiError::NotHeld),
        ("negative", ApiError::NegativeTime),
        ("kind", ApiError::UnknownSlotKind),
        ("share", ApiError::NotAFraction),
    ];
    for (value, error) in refused {
        game.tick(&[(0, input("probe", value))]);
        assert_eq!(game.failures(), [FailureKind::Api(error)], "{value}");
    }
}
