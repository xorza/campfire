use campfire_math::{Rng, RngStream};
use campfire_sim::SimRng;

use super::*;

/// The mode's sequence of the tick `game` just ran, as a call draws from it.
fn mode_sequence(game: &Match) -> Rng {
    let opener = game.resource::<SimRng>().opener();
    opener.open(RngStream::new("script.mode"), StableId::MATCH)
}

#[test]
fn the_modes_draws_follow_on_through_its_calls_in_a_tick_and_start_again_in_the_next() {
    let drawer = r#"
fn take_input(ctx, player, name, value) {
    if value == "draw" {
        ctx.state.kind += `${ctx.pick([10, 20, 30])} ${ctx.chance(num(1) / 2)};`;
    } else if value == "below" {
        ctx.chance(-1);
    } else if value == "above" {
        ctx.chance(num(3) / 2);
    } else if value == "edges" {
        ctx.state.kind = `${ctx.chance(0)} ${ctx.chance(1)}`;
    } else if value == "empty" {
        ctx.pick([]);
    }
}
"#;
    let mut game = picking_match(drawer, ScriptLimits::ROOMY);
    let entries = [10, 20, 30];
    let draw = |rng: &mut Rng| format!("{} {};", entries[rng.pick(3)], rng.chance(Num::HALF));
    // Two calls in one tick draw on from one sequence; the next tick's call from a new one.
    Mode::tick_inputs(
        &mut game,
        &[(0, input("probe", "draw")), (1, input("probe", "draw"))],
    );
    let mut rng = mode_sequence(&game);
    let first = format!("{}{}", draw(&mut rng), draw(&mut rng));
    assert_eq!(
        Mode::field(&game, "kind"),
        StateValue::Text(first.as_str().into())
    );
    Mode::tick_inputs(&mut game, &[(0, input("probe", "draw"))]);
    let second = draw(&mut mode_sequence(&game));
    assert_eq!(
        Mode::field(&game, "kind"),
        StateValue::Text(format!("{first}{second}").into())
    );
    assert_eq!(Mode::failures(&game), []);
    // A chance of 0 never comes true, and one of 1 always does.
    Mode::tick_inputs(&mut game, &[(0, input("probe", "edges"))]);
    assert_eq!(
        Mode::field(&game, "kind"),
        StateValue::Text("false true".into())
    );
    // A probability outside 0 to 1, and a pick from nothing, fail the call.
    let refused = [
        ("below", ApiError::NotAProbability),
        ("above", ApiError::NotAProbability),
        ("empty", ApiError::EmptyPick),
    ];
    for (value, error) in refused {
        Mode::tick_inputs(&mut game, &[(0, input("probe", value))]);
        assert_eq!(Mode::failures(&game), [FailureKind::Api(error)], "{value}");
    }
}
