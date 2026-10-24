use campfire_units::{AudienceOwner, AudienceTeam};

use super::*;

#[test]
fn a_call_writes_a_players_state_reads_it_back_and_the_next_call_reads_it() {
    let script = r#"
fn on_input(ctx, player, name, value) {
    if value == "score" {
        player.state.kills += 1;
        player.state.kills += 1;
        ctx.state.count = player.state.kills;
        player.state.title = "veteran";
    } else if value == "read" {
        let first = ctx.players[0];
        ctx.state.seen = first.state.kills;
        ctx.state.kind = first.state.title;
    } else if value == "fail" {
        player.state.kills = 9;
        throw "the call fails";
    } else if value == "member" {
        player.state.title = "legend";
    } else if value == "deaths" {
        player.state[value] = 1;
    }
}
"#;
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    // Each class on each player's entity: `kills` its owner's, `title` all's, `note` no
    // client's, each at its type's first value, an enum's at its first member.
    let classes = |game: &Match, slot| {
        let entity = game.player_entity(PlayerSlot::new(slot)).unwrap();
        let values = |values: &[StateValue]| values.to_vec();
        (
            values(
                game.get::<PlayerState<AudienceOwner>>(entity)
                    .unwrap()
                    .values(),
            ),
            values(
                game.get::<PlayerState<AudienceAll>>(entity)
                    .unwrap()
                    .values(),
            ),
            values(
                game.get::<PlayerState<AudienceNone>>(entity)
                    .unwrap()
                    .values(),
            ),
            values(
                game.get::<PlayerState<AudienceTeam>>(entity)
                    .unwrap()
                    .values(),
            ),
        )
    };
    let text = |text: &str| StateValue::Text(text.into());
    // A title is its member's place: `rookie` 0, `veteran` 1.
    let at = |kills, title: &str| {
        let member = ["rookie", "veteran"].iter().position(|&of| of == title);
        (
            vec![StateValue::Int(kills)],
            vec![StateValue::Member(u8::try_from(member.unwrap()).unwrap())],
            vec![text("")],
            Vec::new(),
        )
    };
    assert_eq!(classes(&game, 0), at(0, "rookie"));
    // Player 0 scores twice in one call, which reads 2 back; player 1's call of the same tick
    // reads player 0's 2 and title, as the first call's writes applied when it ended. Player
    // 1's own state stays as it was.
    Mode::tick_inputs(
        &mut game,
        &[(0, input("probe", "score")), (1, input("probe", "read"))],
    );
    assert_eq!(Mode::failures(&mut game), []);
    assert_eq!(classes(&game, 0), at(2, "veteran"));
    assert_eq!(classes(&game, 1), at(0, "rookie"));
    let fields = ["kind", "seen", "count"].map(|name| Mode::field(&game, name));
    assert_eq!(
        fields,
        [text("veteran"), StateValue::Int(2), StateValue::Int(2)]
    );
    // A call that fails writes nothing; a text no member of the enum, and a field the mode
    // does not declare, fail the call.
    Mode::tick_inputs(
        &mut game,
        &[
            (0, input("probe", "fail")),
            (0, input("probe", "member")),
            (0, input("probe", "deaths")),
        ],
    );
    let refused = [ApiError::NotAMember, ApiError::UnknownState].map(FailureKind::Api);
    assert_eq!(Mode::failures(&mut game)[1..], refused);
    assert_eq!(classes(&game, 0), at(2, "veteran"));
}
