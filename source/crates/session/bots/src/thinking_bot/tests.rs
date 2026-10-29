use campfire_capabilities::Order;
use campfire_harness::{FixedMatch, FixedSession};
use campfire_package::{LoadProblem, ModePackages, ScriptProblem};
use campfire_script::{ApiError, FailureKind};
use campfire_test_packages::{Edit, PackageTree, TestMode};
use campfire_visibility::DataKind;

use super::*;
use crate::bot_script::BotScript;
use crate::bot_script::tests::Heard;
use crate::order_script::OrderScript;

/// Proving mode with the bot `walker` of `script`, which thinks every 200 ms and gives its own
/// param `hero`, the lancer, over the mode's params.
fn proving(script: &str) -> PackageTree {
    let bots = "\n[bots.walker]\nscript = \"scripts/bot.rhai\"\nthink_ms = 200\n\
                params = { hero = \"hero-lancer\" }\n";
    let mut tree = PackageTree::of(TestMode::Proving);
    tree.edit("modes/proving/data/mode.toml", Edit::Append(bots))
        .edit("modes/proving/scripts/bot.rhai", Edit::Create(script));
    tree
}

/// What one think of the bot `walker` of `packages`, for player slot 0 of `fixed`, from a budget of
/// `operations`, gave; or why it gave nothing.
fn think(
    packages: &ModePackages,
    fixed: &mut FixedMatch,
    operations: u64,
) -> Result<Heard, ThinkError> {
    let walker = DeclaredName::new("walker").unwrap();
    let bot = ThinkingBot::new(packages.bot(&walker).unwrap());
    let books = packages.load_books();
    let names = BotNames::new(books);
    let mut view = HeldBotView::new(fixed.game_mut());
    let mut heard = Heard::default();
    let think = Think {
        player_slot: PlayerSlot::new(0),
        now: Tick::new(1),
        budget: &mut Budget::new(operations),
        names: &names,
        view: &mut view,
        buffers: &mut BotBuffers::default(),
        sink: &mut heard,
    };
    bot.think(fixed.game_mut(), think)?;
    Ok(heard)
}

/// What the order script `text` gives in its tick 0 for player slot 0 of `fixed`.
fn scripted(packages: &ModePackages, fixed: &mut FixedMatch, text: &str) -> Heard {
    let books = packages.load_books();
    let names = BotNames::new(books);
    let mut script = BotScript::new(&OrderScript::parse(text).unwrap(), &names).unwrap();
    let mut view = HeldBotView::new(fixed.game_mut());
    let view = view.get(fixed.game());
    let mut heard = Heard::default();
    script.step(
        Tick::new(0),
        PlayerSlot::new(0),
        &view,
        &mut BotBuffers::default(),
        &mut heard,
    );
    heard
}

#[test]
fn a_bots_think_reads_what_it_sees_and_orders_as_an_order_script_does() {
    // The bot reads its own hero, by its param `hero`, its barracks, the mode's param `income`
    // beneath its own, its player's hero choice, the map's three barracks markers, and the enemy
    // heroes its hero's team sees within 50 m: in tick 0 each team sees its own alone. It moves
    // its hero to a point of those counts, (1 hero + 2 barracks, 100 / 100), and trains a guard
    // at its barracks, and a ward at the second marker, a point as a build takes; an order script
    // of the same three orders, the move to its avatar, the hero, gives the same three payloads.
    let script = r#"
fn on_bot_think(ctx) {
    let me = ctx.player;
    let heroes = ctx.own_units(ctx.params.hero);
    let barracks = ctx.own_units("barracks");
    let chose = ctx.chosen(me, "hero");
    let markers = ctx.map.markers("barracks");
    let seen = ctx.find_visible(heroes[0], heroes[0].position, 50, "enemies:avatar");
    if chose != ["hero-lancer"] || markers.len() != 3 || seen.len() != 0 {
        return;
    }
    let x = heroes.len() + barracks.len();
    ctx.order(heroes, #{ orders: #{ move: #{ to: [x, ctx.params.income / 100] } } });
    ctx.order(barracks, #{ orders: #{ slot: #{ slot: "train_guard", target: "none" } } });
    let ward = #{ point: #{ marker: markers[1].name } };
    ctx.order(barracks, #{ orders: #{ slot: #{ slot: "post_ward", target: ward } } });
}
"#;
    let tree = proving(script);
    let (packages, mut fixed) = (
        tree.packages(),
        FixedSession::new(tree.packages(), 2).start(),
    );
    let thought = think(&packages, &mut fixed, 100_000).unwrap();
    let orders = "[[order]]\ntick = 0\ndirective.orders.move.to = [3, 1]\n\
                  [[order]]\ntick = 0\nunits = \"barracks\"\n\
                  directive.orders.slot = { slot = \"train_guard\", target = \"none\" }\n\
                  [[order]]\ntick = 0\nunits = \"barracks\"\ndirective.orders.slot = \
                  { slot = \"post_ward\", target.point = { marker = \"north_barracks_east\" } }\n";
    let expected = scripted(&packages, &mut fixed, orders);
    assert_eq!(thought.sent.len(), 3);
    assert_eq!(thought.sent, expected.sent);
    assert_eq!(thought.unsent, []);
}

#[test]
fn a_bots_think_fails_on_what_is_no_unit_no_directive_not_its_own_or_past_its_budget() {
    // A list with a number in it, a table of no directive, a read of the other player's gold,
    // which its player's client does not receive, and an order to the allies it sees, not all its
    // own, as `can_start` and `is_idle` refuse one, each fail the think, which gives no order.
    for (body, error) in [
        ("ctx.order([1], #{ orders: \"stop\" });", ApiError::NotAUnit),
        (
            "ctx.order(ctx.own_units(\"hero-lancer\"), #{ orders: #{ fly: 1 } });",
            ApiError::NotADirective,
        ),
        (
            "ctx.players[1].resource(\"gold\");",
            ApiError::Unreceived(DataKind::Resources),
        ),
        (
            "let h = ctx.own_units(ctx.params.hero)[0]; \
             ctx.order(ctx.find_visible(h, h.position, 500, \"allies\"), #{ orders: \"stop\" });",
            ApiError::NotForRole,
        ),
    ] {
        let script = format!("fn on_bot_think(ctx) {{ {body} }}");
        let tree = proving(&script);
        let (packages, mut fixed) = (
            tree.packages(),
            FixedSession::new(tree.packages(), 2).start(),
        );
        let failed = think(&packages, &mut fixed, 100_000).unwrap_err();
        let ThinkError::Call(failed) = failed else {
            panic!("{body}: {failed}");
        };
        assert_eq!(failed.kind(), FailureKind::Api(error), "{body}");
    }
    // A think whose budget its loop spends finds the budget spent, which its driver answers by
    // thinking again in the next tick; the same think from a budget that holds it gives its
    // order.
    let script = "fn on_bot_think(ctx) { for i in 0..20 {} ctx.order(ctx.own_units(\"barracks\"), \
                  #{ orders: \"stop\" }); }";
    let tree = proving(script);
    let (packages, mut fixed) = (
        tree.packages(),
        FixedSession::new(tree.packages(), 2).start(),
    );
    let spent = think(&packages, &mut fixed, 10).unwrap_err();
    assert!(
        matches!(spent, ThinkError::Call(CallError::BudgetSpent)),
        "{spent}"
    );
    let held = think(&packages, &mut fixed, 100_000).unwrap();
    assert_eq!(held.sent.len(), 1);
}

#[test]
fn a_bot_script_may_use_only_what_a_bots_think_may() {
    // A bot's script that spawns, or reads the avatars, names what no bot's call may, and fails
    // the load.
    for (body, name) in [
        (
            "ctx.spawn_unit(\"guard\", \"north\", vector(0, 0, 0));",
            "spawn_unit",
        ),
        ("ctx.avatars();", "avatars"),
    ] {
        let script = format!("fn on_bot_think(ctx) {{ {body} }}");
        let error = proving(&script).load().unwrap_err();
        let LoadProblem::Script {
            problem: ScriptProblem::UnknownCtx(unknown),
            ..
        } = *error.problem
        else {
            panic!("{body}: {error}");
        };
        assert_eq!(unknown, name, "{body}");
    }
}

#[test]
fn a_bots_think_reads_what_its_units_may_start() {
    // The bot stops its barracks when they may train a guard, for its player's 100 gold, and its
    // hero may not start its first ability, `fan`, as a think lists trains and builds alone.
    // Before the first tick no holdings count the units, so nothing may start; after it, the
    // stop goes to both barracks in one order.
    let script = r#"
fn on_bot_think(ctx) {
    let barracks = ctx.own_units("barracks");
    let hero = ctx.own_units(ctx.params.hero)[0];
    if ctx.can_start(barracks[0], "train_guard") && !ctx.can_start(hero, "fan") {
        ctx.order(barracks, #{ orders: "stop" });
    }
}
"#;
    let tree = proving(script);
    let (packages, mut fixed) = (
        tree.packages(),
        FixedSession::new(tree.packages(), 2).start(),
    );
    assert_eq!(think(&packages, &mut fixed, 100_000).unwrap().sent.len(), 0);
    fixed.step();
    assert_eq!(think(&packages, &mut fixed, 100_000).unwrap().sent.len(), 1);
}

#[test]
fn bots_of_one_period_think_each_in_ticks_of_its_own_player_slot() {
    // Every 3 ticks: player slot 0 in ticks 0, 3 and 6, player slot 1 a tick after each, in 1, 4
    // and 7, and player slot 4 a whole period after player slot 1, in its ticks from its own, 4 and
    // 7; every tick, player slot 1 in every tick from its own.
    let thinks = |player_slot: u32, period: u64| {
        let period = Ticks::new(period);
        (0..9)
            .filter(|&tick| {
                PacedBot::in_phase(PlayerSlot::new(player_slot), Tick::new(tick), period)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(thinks(0, 3), [0, 3, 6]);
    assert_eq!(thinks(1, 3), [1, 4, 7]);
    assert_eq!(thinks(4, 3), [4, 7]);
    assert_eq!(thinks(1, 1), (1..9).collect::<Vec<_>>());
}

#[test]
fn a_paced_bot_thinks_in_its_phase_and_waits_out_a_spent_budget() {
    // The walker thinks every 200 ms, 4 ticks of the proving mode's 20 Hz; player slot 0 in ticks
    // 0, 4 and 8. Out of phase, tick 1 gives nothing. In tick 4 a budget of 10 operations is spent
    // before its loop ends: no order, and the bot waits, so it thinks in tick 5, out of its phase,
    // and gives its stop; in tick 6 it is out of phase again.
    let script = "fn on_bot_think(ctx) { for i in 0..20 {} ctx.order(ctx.own_units(\"barracks\"), \
                  #{ orders: \"stop\" }); }";
    let tree = proving(script);
    let (packages, mut fixed) = (
        tree.packages(),
        FixedSession::new(tree.packages(), 2).start(),
    );
    let walker = DeclaredName::new("walker").unwrap();
    let rate = *fixed.game().resource::<MatchRate>();
    let mut bot = PacedBot::new(ThinkingBot::new(packages.bot(&walker).unwrap()), rate);
    let books = packages.load_books();
    let names = BotNames::new(books);
    let mut view = HeldBotView::new(fixed.game_mut());
    let mut play = |fixed: &mut FixedMatch, tick: u64, operations: u64| {
        let mut heard = Heard::default();
        let think = Think {
            player_slot: PlayerSlot::new(0),
            now: Tick::new(tick),
            budget: &mut Budget::new(operations),
            names: &names,
            view: &mut view,
            buffers: &mut BotBuffers::default(),
            sink: &mut heard,
        };
        bot.play(fixed.game_mut(), think).unwrap();
        heard.sent.len()
    };
    assert_eq!(play(&mut fixed, 1, 100_000), 0);
    assert_eq!(play(&mut fixed, 4, 10), 0);
    assert_eq!(play(&mut fixed, 5, 100_000), 1);
    assert_eq!(play(&mut fixed, 6, 100_000), 0);
}

#[test]
fn a_bots_think_reads_which_of_its_units_stand_idle() {
    // The bot walks its hero to (5, 5) while it stands idle. The first think finds it idle and
    // orders the walk; once the walk applies, the hero has a destination, and the next think
    // finds it busy and orders nothing.
    let script = "
fn on_bot_think(ctx) {
    let hero = ctx.own_units(ctx.params.hero);
    if ctx.is_idle(hero[0]) {
        ctx.order(hero, #{ orders: #{ move: #{ to: [5, 5] } } });
    }
}
";
    let tree = proving(script);
    let (packages, mut fixed) = (
        tree.packages(),
        FixedSession::new(tree.packages(), 2).start(),
    );
    let first = think(&packages, &mut fixed, 100_000).unwrap();
    assert_eq!(first.sent.len(), 1);
    fixed.send(0, Tick::new(0), &Order::payload(&first.sent));
    fixed.step();
    assert_eq!(think(&packages, &mut fixed, 100_000).unwrap().sent.len(), 0);
}
