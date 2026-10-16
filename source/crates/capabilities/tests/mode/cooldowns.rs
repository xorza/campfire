use campfire_abilities::AbilitiesRefusal;
use campfire_actions::SlotTimes;
use campfire_common::Tick;
use campfire_script::CapabilityRefusal;
use campfire_sim::internals::SimWorldInternals;

use super::*;

#[test]
fn a_script_cuts_cooldowns_by_time_and_by_share_down_to_now() {
    let cutter = r#"
fn on_input(ctx, player, name, value) {
    let x = ctx.avatars("a")[0];
    if value == "cut" {
        ctx.reduce_cooldown(x, "strike", 1000);
    } else if value == "part" {
        ctx.reduce_cooldown(x, "strike", 250);
    } else if value == "zero" {
        ctx.reduce_cooldown(x, "strike", 0);
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
    let mut game = picking_match(cutter, ScriptLimits::ROOMY);
    // Hero X holds `strike` in its basic slot 0; hero Y does not hold it.
    let x = Mode::pick(&mut game, 0, "hero-x");
    Mode::pick(&mut game, 2, "hero-y");
    let ready = |game: &Match| {
        let times = game.get::<SlotTimes>(x).unwrap();
        times.slot(0).unwrap().ready_at.get()
    };
    // Ready 50 ticks from the call's tick; 1000 ms is 10 ticks at 10 a second: 40 from then.
    let start = game.now().get();
    game.get_mut::<SlotTimes>(x)
        .unwrap()
        .cool_down(0, Tick::new(start + 50));
    Mode::tick_inputs(&mut game, &[(0, input("probe", "cut"))]);
    assert_eq!(ready(&game), start + 40);
    // A cut rounds down, so it ends no wait early: 250 ms is 2.5 ticks, down to 2; 0 ms cuts
    // nothing.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "part"))]);
    assert_eq!(ready(&game), start + 38);
    Mode::tick_inputs(&mut game, &[(0, input("probe", "zero"))]);
    assert_eq!(ready(&game), start + 38);
    // Three ticks later 35 are left; half of them is 17.5, which rounds up to 18 kept.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "half"))]);
    assert_eq!(ready(&game), start + 3 + 18);
    // A cut past what is left makes it ready in the call's tick, no earlier.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "all"))]);
    assert_eq!(ready(&game), start + 4);
    assert_eq!(Mode::failures(&game), []);
    let refused = [
        ("unknown", ApiError::Unknown(NameKind::Ability)),
        ("unheld", AbilitiesRefusal::NotHeld.refused()),
        ("negative", ApiError::NegativeTime),
        ("kind", ApiError::Unknown(NameKind::SlotKind)),
        ("share", AbilitiesRefusal::NotAFraction.refused()),
    ];
    for (value, error) in refused {
        Mode::tick_inputs(&mut game, &[(0, input("probe", value))]);
        assert_eq!(Mode::failures(&game), [FailureKind::Api(error)], "{value}");
    }
}
