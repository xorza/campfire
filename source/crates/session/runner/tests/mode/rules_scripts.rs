use std::num::NonZeroU64;

use campfire_common::Tick;
use campfire_progression::LevelUps;
use campfire_script::ModeEvents;
use campfire_units::AudienceOwner;

use super::*;

/// A rules script that adds 1 to the first player's kills as the match starts, and sets its
/// `once`, due in a tick, whose call adds 100.
const BEFORE: &str = r#"
fn on_match_start(ctx) {
    ctx.players[0].state.kills += 1;
    ctx.after("once", 100, 1);
}

fn on_timer(ctx, name, data) {
    ctx.players[0].state.kills += 100;
}
"#;

/// The mode's own script: it reads the kills as the match starts, and sets its `once`, due in two
/// ticks, whose call keeps its data.
const OWN: &str = r#"
fn on_match_start(ctx) {
    ctx.state.seen = ctx.players[0].state.kills;
    ctx.after("once", 200, 2);
}

fn on_timer(ctx, name, data) {
    ctx.state.count = data;
}
"#;

/// A rules script that adds 10 as the match starts, then sets its `once` and ends it.
const AFTER: &str = r#"
fn on_match_start(ctx) {
    ctx.players[0].state.kills += 10;
    ctx.after("once", 100, 3);
    ctx.cancel_timer("once");
}
"#;

/// The first player's kills.
fn kills(game: &Match) -> StateValue {
    let entity = game.player_entity(PlayerSlot::new(0)).unwrap();
    game.get::<PlayerState<AudienceOwner>>(entity)
        .unwrap()
        .values()[0]
        .clone()
}

#[test]
fn each_script_of_a_modes_list_answers_in_order_and_owns_its_timers() {
    let mut game = listed_match(&[BEFORE, OWN, AFTER], 1, ScriptLimits::ROOMY).unwrap();
    assert_eq!(Mode::failures(&mut game), []);
    // The script before the mode's own added 1 before it read the kills, and the one after 10
    // after it.
    assert_eq!(Mode::field(&game, "seen"), StateValue::Int(1));
    assert_eq!(kills(&game), StateValue::Int(11));
    // Each set its own `once`, and the last ended only its own: the mode's, due in tick 2, then
    // the first script's, due in tick 1, which fires first.
    let timers = |game: &mut Match| {
        let mut timers = game.resource_mut::<Timers>();
        let held = timers.timers_mut().iter();
        held.map(|timer| (timer.script.index(), timer.due.get()))
            .collect::<Vec<_>>()
    };
    assert_eq!(timers(&mut game), [(1, 2), (0, 1)]);
    // A timer runs `on_timer` of its own script alone: the first script's adds 100, and the
    // mode's own, a tick later, keeps its data.
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(
        (kills(&game), Mode::field(&game, "count")),
        (StateValue::Int(111), StateValue::Int(0))
    );
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(
        (kills(&game), Mode::field(&game, "count")),
        (StateValue::Int(111), StateValue::Int(2))
    );
    assert_eq!(Mode::failures(&mut game), []);
}

#[test]
fn the_modes_own_script_ends_the_match_listed_after_a_rules_script() {
    // The load refuses a rules script that ends the match, chooses or reads the mode's state, so
    // the mode's own script alone ends it, listed after a rules script.
    let ends = "fn on_match_start(ctx) { ctx.win(\"a\"); }";
    let game = listed_match(&["fn on_match_start(ctx) {}", ends], 1, ScriptLimits::ROOMY);
    let won = MatchEnd::new(Tick::new(0), MatchResult::Won(Team::new(0)));
    assert_eq!(game.unwrap().match_end(), Some(won));
}

#[test]
fn an_event_whose_call_finds_the_pool_spent_runs_later_in_the_scripts_that_did_not_answer_it() {
    // Each level-up call spins 50 additions: a mode pool of 300 operations holds one, not two.
    let rules = "
fn on_level_up(ctx, unit, track, level) {
    ctx.players[0].state.kills += 1;
    let spun = 0;
    for i in 0..50 { spun += i; }
}
";
    let own = r#"
fn on_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
    } else {
        ctx.add_xp(ctx.avatars()[0], "level", 100);
    }
}

fn on_level_up(ctx, unit, track, level) {
    ctx.state.count += 1;
    let spun = 0;
    for i in 0..50 { spun += i; }
}
"#;
    let limits = ScriptLimits {
        per_call: NonZeroU64::new(300).unwrap(),
        mode: 300,
        ..ScriptLimits::ROOMY
    };
    let mut game = listed_match(&[rules, own], 1, limits).unwrap();
    Mode::tick_inputs(&mut game, &[(0, input("hero", "hero-x"))]);
    Mode::tick_inputs(&mut game, &[(0, input("phase", ""))]);
    // The rules script answered the level-up; the mode's own call found the pool spent, and the
    // level-up waits with one script answered.
    let state = |game: &Match| {
        let partial = game.resource::<LevelUps>().partial();
        (kills(game), Mode::field(game, "count"), partial)
    };
    assert_eq!(state(&game), (StateValue::Int(1), StateValue::Int(0), 1));
    // The next tick runs it in the mode's own script alone.
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(state(&game), (StateValue::Int(1), StateValue::Int(1), 0));
    assert!(game.resource::<LevelUps>().is_empty());
}
