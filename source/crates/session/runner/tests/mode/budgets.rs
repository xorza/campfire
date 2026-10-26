use std::num::NonZeroU64;

use campfire_codec::{Binary, BinaryError};
use campfire_combat::{Kept, UnansweredDeaths};
use campfire_common::Tick;
use campfire_progression::LevelUps;

use super::*;

#[test]
fn a_player_spends_only_their_own_budget() {
    // Each player's budget holds one whole call of 1000: player 0's spin runs its 1000 and fails,
    // which spends its budget, so its next input fails unrun; player 1's input in the same tick
    // runs from its own budget.
    let spin = r#"
fn on_input(ctx, player, name, value) {
    ctx.state.inputs += 1;
    if name == "fail" {
        loop {}
    }
}
"#;
    let limits = ScriptLimits {
        per_call: NonZeroU64::new(1000).unwrap(),
        player: 1000,
        ..ScriptLimits::ROOMY
    };
    let mut game = mode_match(spin, limits);
    let inputs = |game: &Match| Mode::field(game, "inputs");
    Mode::tick_inputs(
        &mut game,
        &[
            (0, input("fail", "")),
            (0, input("phase", "")),
            (1, input("phase", "")),
        ],
    );
    let failures = game.resource_mut::<ScriptFailures>().take_calls();
    let errors: Vec<_> = failures.iter().map(|failure| failure.kind).collect();
    assert_eq!(errors, [FailureKind::CallLimit, FailureKind::BudgetSpent]);
    assert_eq!(inputs(&game), StateValue::Int(1));
    // Every budget starts full in the next tick.
    Mode::tick_inputs(&mut game, &[(0, input("phase", ""))]);
    assert_eq!(inputs(&game), StateValue::Int(2));
}

#[test]
fn a_timer_whose_call_finds_the_mode_budget_spent_stays_due() {
    // A budget of 1500 operations: the first spinning call runs its 1000 and fails, which leaves
    // 500; the second ends past those, and its timer waits for the next tick.
    let spin = r#"
fn on_match_start(ctx) {
    ctx.after("a", 100);
    ctx.after("b", 100);
}

fn on_timer(ctx, name, data) {
    ctx.state.count += 1;
    loop {}
}
"#;
    let limits = ScriptLimits {
        per_call: NonZeroU64::new(1000).unwrap(),
        mode: 1500,
        ..ScriptLimits::ROOMY
    };
    let mut game = mode_match(spin, limits);
    let due = |game: &Match| {
        let timers = game.resource::<Timers>();
        (
            timers.due(Tick::new(u64::MAX)).map(|timer| {
                let names = game.resource::<MatchNames>().table::<TimerId>();
                names.unwrap().name(timer.name).unwrap().to_owned()
            }),
            timers.due(Tick::ZERO).is_some(),
        )
    };
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(due(&game), (Some("b".to_owned()), false));
    assert_eq!(Mode::failures(&mut game), [FailureKind::CallLimit]);
    Mode::tick_inputs(&mut game, &[]);
    assert_eq!(due(&game), (None, false));
    assert_eq!(Mode::failures(&mut game), [FailureKind::CallLimit]);
    // Both calls failed, so none counted.
    assert_eq!(Mode::field(&game, "count"), StateValue::Int(0));
}

#[test]
fn a_death_or_a_level_up_whose_call_finds_the_mode_budget_spent_waits_with_its_unit() {
    // Each death call spins 50 additions: a mode budget of 500 operations holds two of them, not
    // three, and then the third and a level-up.
    let script = r#"
fn on_match_start(ctx) {
    ctx.spawn_group("b", "mid", PathEnd::End, ["grunt", "grunt", "grunt"]);
}

fn on_input(ctx, player, name, value) {
    if name == "hero" {
        pick(ctx, player, value);
    } else {
        ctx.add_xp(ctx.avatars()[0], "level", 100);
    }
}

fn on_unit_death(ctx, unit, killer, assisters) {
    ctx.state.kind += `${unit.team} by ${killer.team};`;
    let spun = 0;
    for i in 0..50 { spun += i; }
}

fn on_level_up(ctx, unit, track, level) {
    ctx.state.kind += `${track} ${level};`;
}
"#;
    let limits = ScriptLimits {
        per_call: NonZeroU64::new(500).unwrap(),
        mode: 500,
        ..ScriptLimits::ROOMY
    };
    // The grunts despawn when they die.
    let mut set = mode_packages(script);
    let kept = "move_speed = { base = 10 } }\ncombat = { on_death = \"stay\" }\nstate";
    let despawn = "move_speed = { base = 10 } }\ncombat = { on_death = \"despawn\" }\nstate";
    set.edit(UNITS, Edit::Replace(kept, despawn));
    let mut game = started(&set, limits).unwrap();
    Mode::tick_inputs(&mut game, &[(0, input("hero", "hero-x"))]);
    // Units: the tower 0 of a, b's grunts 1 to 3, which despawn when they die, and the hero 4.
    // The tower's damage kills the three grunts in tick 1, as the hero reaches level 2.
    let grunts = [1, 2, 3].map(|at| Mode::entity_by_id(&game, at));
    let tower = game.get::<StableId>(Mode::entity_by_id(&game, 0)).copied();
    for grunt in grunts {
        let target = *game.get::<StableId>(grunt).unwrap();
        DamagePass::queue(
            &mut game,
            PassEntry::Damage(Damage {
                source: tower,
                target,
                amount: Num::int(10),
                kind: DamageKind::new(0),
                cause: DamageCause::Effect,
                action: None,
                depth: 0,
                hit: None,
            }),
        );
    }
    let waiting = |game: &Match| {
        let deaths = game.resource::<UnansweredDeaths>().iter().count();
        (deaths, game.resource::<LevelUps>().0.len())
    };
    let stands = |game: &Match, grunt: Entity| {
        let unit = game.get_entity(grunt).ok()?;
        Some((unit.contains::<Dead>(), unit.contains::<Kept>()))
    };
    Mode::tick_inputs(&mut game, &[(0, input("probe", "xp"))]);
    // Two deaths ran; the third waits, and so does the level-up behind it. The first two grunts
    // despawned; the third stays, dead and kept, for its call.
    assert_eq!(
        Mode::field(&game, "kind"),
        StateValue::Text("b by a;b by a;".into())
    );
    assert_eq!(waiting(&game), (1, 1));
    // The waiting death is state: it decodes to itself, and an assister its runs do not cover
    // fails to decode. Its assisters come last, none: their length is the last byte.
    let unanswered = game.resource::<UnansweredDeaths>();
    let mut bytes = Binary::encode(unanswered);
    let decoded = Binary::decode::<UnansweredDeaths>(&bytes).ok();
    assert_eq!(decoded.as_ref(), Some(unanswered));
    assert_eq!(bytes.pop(), Some(0));
    bytes.extend([1, 5]);
    assert_eq!(
        Binary::decode::<UnansweredDeaths>(&bytes).err(),
        Some(BinaryError::REFUSED)
    );
    assert_eq!(
        grunts.map(|grunt| stands(&game, grunt)),
        [None, None, Some((true, true))]
    );
    Mode::tick_inputs(&mut game, &[]);
    // Both run first in the next tick, with the third grunt and its killer as handles; then it
    // despawns.
    let all = "b by a;b by a;b by a;level 2;";
    assert_eq!(Mode::field(&game, "kind"), StateValue::Text(all.into()));
    assert_eq!(waiting(&game), (0, 0));
    assert_eq!(stands(&game, grunts[2]), None);
    assert_eq!(Mode::failures(&mut game), []);
}

#[test]
fn a_join_or_a_leave_whose_call_finds_the_mode_budget_spent_waits() {
    // Each call spins 50 additions: a mode budget of 500 operations holds two of them, not three.
    let script = "
fn on_player_join(ctx, player) {
    ctx.state.kind += `join ${player.player_slot};`;
    let spun = 0;
    for i in 0..50 { spun += i; }
}

fn on_player_leave(ctx, player) {
    ctx.state.kind += `leave ${player.player_slot};`;
    let spun = 0;
    for i in 0..50 { spun += i; }
}
";
    let limits = ScriptLimits {
        per_call: NonZeroU64::new(500).unwrap(),
        mode: 500,
        ..ScriptLimits::ROOMY
    };
    let mut game = mode_match(script, limits);
    // Player 2 leaves, and players 1 and 2 join, in one tick: the first two calls run, in that
    // order, and player 2's join waits.
    let (joined, left) = (PlayerSlotEventKind::Joined, PlayerSlotEventKind::Left);
    Mode::tick_player_slots(&mut game, &[(2, left), (1, joined), (2, joined)]);
    assert_eq!(
        Mode::field(&game, "kind"),
        StateValue::Text("leave 2;join 1;".into())
    );
    let waiting = game.resource::<UnansweredPlayerSlotEvents>();
    let join = PlayerSlotEvent {
        player_slot: PlayerSlot::new(2),
        kind: joined,
    };
    assert_eq!(waiting.0.iter().collect::<Vec<_>>(), [&join]);
    // The waiting join is state: it decodes to itself.
    let bytes = Binary::encode(waiting);
    let decoded = Binary::decode::<UnansweredPlayerSlotEvents>(&bytes).ok();
    assert_eq!(decoded.as_ref(), Some(waiting));
    // It runs first in the next tick, before that tick's leave of player 1.
    Mode::tick_player_slots(&mut game, &[(1, left)]);
    let all = "leave 2;join 1;join 2;leave 1;";
    assert_eq!(Mode::field(&game, "kind"), StateValue::Text(all.into()));
    assert!(game.resource::<UnansweredPlayerSlotEvents>().0.is_empty());
}

#[test]
fn a_method_of_a_value_read_through_a_field_costs_no_write_back() {
    // `ctx.map.markers("spawn")` reads the map through its field, then calls a method of it, as
    // 8 operations: the statement, its expression, the chain's two links of arguments, the
    // literal, `ctx`, the getter of `map` and `markers`. The method takes the map as a copy, so
    // Rhai does not feed it back through a setter of `map`, which `Ctx` lacks, and then through
    // an indexer: those would cost one operation more each, 10 in all.
    let script = r#"
fn on_input(ctx, player, name, value) {
    if name == "rich" {
        ctx.map.markers("spawn");
        ctx.map.markers("spawn");
    } else {
        ctx.map.markers("spawn");
    }
}
"#;
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    let mut spent = |name: &str| {
        Mode::tick_inputs(&mut game, &[(0, input(name, ""))]);
        let mut budgets = game.resource_mut::<ScriptBudgets>();
        ScriptLimits::ROOMY.player - budgets.get_mut(BudgetOf::Player(PlayerSlot::new(0))).left()
    };
    let (once, twice) = (spent("phase"), spent("rich"));
    assert_eq!(twice - once, 8);
}

#[test]
fn a_list_of_the_view_costs_a_step_for_each_unit_it_visits() {
    // The same list, of every unit tagged `grunt`, read before and after five grunts spawn: it
    // visits every unit of the view, five more the second time, so it costs five more
    // operations (02-engine-core.md, A list costs a step a row).
    let script = r#"
fn on_input(ctx, player, name, value) {
    if name == "rich" {
        for count in 0..5 {
            ctx.spawn_unit("grunt", "a", ctx.map.markers("camp")[0].position);
        }
    } else {
        ctx.units_tagged("grunt");
    }
}
"#;
    let mut game = mode_match(script, ScriptLimits::ROOMY);
    let spent = |game: &mut Match, name: &str| {
        Mode::tick_inputs(game, &[(0, input(name, ""))]);
        assert_eq!(Mode::failures(game), [], "{name}");
        let mut budgets = game.resource_mut::<ScriptBudgets>();
        ScriptLimits::ROOMY.player - budgets.get_mut(BudgetOf::Player(PlayerSlot::new(0))).left()
    };
    let few = spent(&mut game, "phase");
    spent(&mut game, "rich");
    let more = spent(&mut game, "phase");
    assert_eq!(more - few, 5);
}
