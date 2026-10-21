use campfire_abilities::AbilitiesRefusal;
use campfire_actions::SlotTimes;
use campfire_common::Tick;
use campfire_script::CapabilityRefusal;
use campfire_sim::internals::SimWorldInternals;
use campfire_values::Named;

use super::*;

#[test]
fn a_script_cuts_cooldowns_by_time_and_by_share_down_to_now() {
    let cutter = r#"
fn take_input(ctx, player, name, value) {
    let x = ctx.all_units_tagged("striker")[0];
    if value == "cut" {
        ctx.cut_cooldown(x, "strike", 1000);
    } else if value == "part" {
        ctx.cut_cooldown(x, "strike", 250);
    } else if value == "zero" {
        ctx.cut_cooldown(x, "strike", 0);
    } else if value == "half" {
        ctx.scale_cooldowns(x, "basic", num(1) / 2);
    } else if value == "all" {
        ctx.cut_cooldown(x, "strike", 100000);
    } else if value == "unknown" {
        ctx.cut_cooldown(x, "lunge", 1000);
    } else if value == "unheld" {
        ctx.cut_cooldown(ctx.all_units_tagged("tower")[0], "strike", 1000);
    } else if value == "negative" {
        ctx.cut_cooldown(x, "strike", -1);
    } else if value == "kind" {
        ctx.scale_cooldowns(x, "ultimate", 1);
    } else if value == "share" {
        ctx.scale_cooldowns(x, "basic", 2);
    }
}
"#;
    // A striker, a unit type of the mode's own, holds the mode's `strike` in its basic slot 0, as
    // a script cuts the cooldown of an action of its own package alone; the tower holds none.
    let mut set = mode_packages(&format!("{cutter}{PICKING}"));
    set.data
        .push_str("\n[tags.striker]\n\n[actions.strike]\ntargeting = \"none\"\n");
    let striker = "\n[units.striker]\ntags = [\"striker\"]\nslots = { basic = [\"strike\"] }\n";
    set.units.push_str(striker);
    let mut game = started(&set, ScriptLimits::ROOMY).unwrap();
    let x = spawn_of(&mut game, "striker", Team::new(0));
    let x = game.unit_entity(x);
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
        ("unknown", ApiError::Unknown(ActionId::NAMES)),
        ("unheld", AbilitiesRefusal::NotHeld.refused()),
        ("negative", ApiError::NegativeTime),
        ("kind", ApiError::Unknown(SlotKind::NAMES)),
        ("share", AbilitiesRefusal::NotAFraction.refused()),
    ];
    for (value, error) in refused {
        Mode::tick_inputs(&mut game, &[(0, input("probe", value))]);
        assert_eq!(Mode::failures(&game), [FailureKind::Api(error)], "{value}");
    }
}
