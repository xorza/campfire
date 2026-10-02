# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

The structural redesign comes first, in the order below, as [Structural rules](design/02-engine-core.md#structural-rules) asks; each step's shape moves into the design when the step lands. Each step ends with the check chain, both goldens and the structure tests that exist then; a step that changes behaviour names the change. The game model's own steps sit where the redesign makes room for them.

The review of the redesign's diff found two defects and two weak shapes; K1 to K4 close them before F3. A second review found four more defects, a fact with two owners and a small flaw; K5 to K10 close them, also before F3.

The workspace and test reviews leave four changes. V1 moves the load's flaws onto the test packages before K1, as K1, K6 and K8 add flaws and this stage rewrites the reference packages step by step. V2 to V4 follow K10. The review's other two items need no code: the state hash costs nothing in a match, which hashes at checkpoints and at the result alone, and an empty tick is the 3v3's own work, its AIs' thinks and its towers' sight, not idle systems.

1. **V1a: the mode's load flaws edit the proving mode** ([Testing](design/02-engine-core.md#testing-and-diagnostics)). Size M.
   - Today the 184 flaws of `mode_package.rs` edit the reference 3v3, six of its heroes and its loadout. A change of a name, a key, a script's text or the map in the reference game breaks flaws that test the loader, not the game.
   - The flaws of the mode's files (manifest, mode data, units, map, mode scripts: 97 flaws) edit `test-proving`, which the tests own. A flaw that needs a section the proving packages lack adds it with its `also` edits, so the proving match and its goldens do not change.
   - Tests: each moved flaw fails with the same problem as before.
2. **V1b: the heroes' load flaws and the text tests edit the proving heroes**. Size M.
   - The flaws of the heroes' files (data, manifests, scripts, text: 86 flaws) edit `hero-lancer` and `hero-sage`, as V1a does, and the text tests of `texts.rs` do too. The one flaw of a loadout creates its loadout package with `Edit::Create`, and adds it to the proving mode's dependencies. `moba.rs` keeps the reference packages' own tests: they load, with their texts.
   - Tests: each moved flaw and text test fails, or passes, as before; no flaw names a reference package.
3. **K1: every way that applies a modifier gives its params** ([Stats](design/04-capabilities/stats.md#modifiers), Appliers). Changes behaviour. Size M.
   - Today the load accepts a param when one applier declares it, and an application that does not resolve returns with no error: `ctx.add_modifier(target, "slow", 1000)` in the lancer's `quake.rhai` loads and does nothing. `has_modifier` counts as an application.
   - The registry marks a modifier argument by its way: `add_modifier` applies with the call's action, `add_player_modifier` with none, and `has_modifier` only names the modifier. The existence check takes all three.
   - `ModifierWays`, in its own file in the package crate, finds each modifier's ways from data and from the scripts' literal names, with auras and modifier scripts passing on the ways of their modifier until nothing changes. It replaces `Package::appliers`, and `LoadCheck::modifiers` and `ModePackages::stat_graph` read it. Each refusal is a `LoadProblem` that names the modifier, the param and the way.
   - A modifier's entry keeps the applier params its numbers read and the ranks its own per-rank params hold. `ctx.add_modifier` and `ctx.add_player_modifier` fail the call when the call's way lacks one.
   - A scaling param's sum stops at the end of the number range. With every way checked, `ModifierBook::application` cannot fail, and the three returns that drop an application go. `CallError::ParamOverflow` goes when nothing raises it.
   - Tests: a flaw for each refusal (an action that lacks the param, a way with no action, an aura's and a modifier script's ways passed on, a short own per-rank param, a time that reads a scaling param, `has_modifier` as no way); a call with a computed name; the saturated sum's exact values.
   - The change of behaviour: a cast whose scaling param leaves the number range plays with the range's end, where the call failed. Reference content that breaks the rule is fixed in the step, and named.
4. **K2: restored times and counts stay within what a match makes** ([Structural rules](design/02-engine-core.md#structural-rules)). Size M.
   - Today a restore accepts values that panic or hang later: an interval of `u64::MAX` ticks overflows in `ModifierClocks::advance_intervals`; a `stack_life` of `u64::MAX` overflows in `Modifiers::set_stacks`; a repeating timer of 0 ticks loops in `run_timers`, and one of `u64::MAX` overflows in `Timers::fire`; a `next_seq` or `IdAllocator` at `u64::MAX` overflows at its next use; an attack under way that resolves sooner than its windup underflows in `strike`.
   - One limit, `Tick::LIMIT`, 2⁶². Every sum and difference a system takes of a restored value is listed, and the check of its type refuses what breaks it: the tick, times and counts above the limit, a period of 0, and each relation such as the attack's.
   - Tests: a restore test per state type at its limits: the value at the limit restores and plays a tick, and the value past it is refused; the cases above among them.
5. **K3: a book error names its place by type**. Size S.
   - Today `BookError` holds its names as strings, and `LoadCheck::book_error` parses them again and takes a unit type for the avatar when its name is the package's: an avatar package `x` with a delivery type `x` whose area time does not count gets the avatar's place.
   - `BookError` holds a `DeclaredName` for an action, a modifier and a delivery type, and a unit type that stands as the package's avatar or a declared name. `LoadCheck::book_error` maps each to its `Place` with no comparison and no parse.
   - Test: the case above names the delivery type.
6. **K4: each effect type applies itself** ([Scripting](design/02-engine-core.md#scripting)). Size S.
   - Today an effect type's `CAPABILITY`, its row in `CAPABILITIES` and the type its row's `apply_next` takes are three facts that must agree, and a mismatch panics at the first effect.
   - `Effect` loses `CAPABILITY` and gains `apply(self, world, frame, now)`. `Effects` finds a type's queue by its `TypeId`, as `CallParts` finds a part, and records with each effect the apply of its type. The table's effect column, `DISPATCH`, `Frame::set_dispatch` and the seven `apply_next` go.
   - Test: the effects test with two types of one capability.
7. **K5: an `xp` effect follows the rule of `ctx.add_xp`** ([Progression](design/04-capabilities/progression.md#rules)). Changes behaviour. Size S.
   - Today the effect `xp = { track, amount }` queues with no check, and `Progression::apply` expects the unit and its track: an `xp` effect to a reached unit with no tracks panics in `resolve_casts`, and one to the source panics once the source despawned before its delivery hits. Its amount is never negative: `Amount::number` gives 0 for a scaling param below zero.
   - Design 04 says a unit without the track fails the call, for the effect as for `ctx.add_xp`. `EffectLists::queue` checks each `xp` effect against the view as `add_xp` does: a unit the view does not hold, or a unit without the track, fails the call with `NoTrack`, and the call changes nothing, its list included. The `expect`s of `Progression::apply` then hold for every effect that reaches it.
   - Tests: an `xp` effect to a reached unit with no tracks, and to a source that despawned before its projectile hits: each fails its call, records its failure, and changes no unit.
   - The change of behaviour: these calls panicked, and now fail.
8. **K6: a modifier is the passive of one owner** ([Stats](design/04-capabilities/stats.md#rules), Passives). Changes behaviour. Size S.
   - Today the passive hold is keyed by the modifier and its source, as every hold of one modifier from one source is one hold. Two actions of one package with one `passive_modifier`, or an avatar's `passive` that one of its actions also names, share it: an unlearned slot releases what a learned slot holds, so the unit has no passive, and two slots at other ranks apply it again in each run of `hold_passives`, three a tick, which resets its shield, interval and state.
   - A passive names a modifier of its own package, so the load refuses a package where a modifier is the passive of more than one owner: a unit type's `passive`, or an action's `passive_modifier`. The refusal is a `LoadProblem` that names the modifier and both owners.
   - Tests: a flaw for each pair (two actions; a unit type and an action).
   - The change of behaviour: such a package loads no more. Reference content that breaks the rule is fixed in the step, and named.
9. **K7: a restore refuses a state a system panics on** ([Structural rules](design/02-engine-core.md#structural-rules)). After K2. Size M.
   - Today three flaws restore and then panic or act: a `Route` that waits for an answer with no goal panics in `plan_routes`; a position within the world's bound but outside the map's panics in vision, as no check reads the map's bounds; and a modifier's clock whose interval or shield its modifier does not have reaches the `debug_assert` of `ModifierClocks::renew`, or fires an interval the modifier lacks. The snapshot fuzz flips one byte at a time, so it makes none of them.
   - `Route` holds the tick it asked in with its goal, `goal: Option<Goal { at, asked }>`, so an ask with no goal cannot be expressed.
   - The state registry takes a check of a state type from a capability that does not own it: `Units` checks each position against the map's bounds, which the sim crate does not know.
   - `ModifierClocks::check` compares each clock with its modifier's spec: an interval and a shield exactly when the spec has one.
   - Tests: a restore test of each flaw above; and a structured fuzz beside the byte flips, which writes into a proving match's snapshot, for each state type, values that pass its decode, restores it, and plays five ticks on what restores.
   - Design 02's rule of restored state names the structured fuzz among what enforces it.
10. **K8: a modifier's aura radius and shield are never negative** ([Stats](design/04-capabilities/stats.md#rules), Numbers). Changes behaviour. Size S.
    - Today the load accepts a negative aura `radius` or `shield`, as a value or a param, the application keeps it, and the decode of `Modifiers` and `ModifierClocks` refuses it: a match makes a state its restore refuses.
    - The load refuses a negative value, and a per-rank param that is negative at a rank, as it does for an effect's numbers. A scaling param that gives a negative value at an application gives 0, so the sim and the decode keep one rule, as `Area::new` and `MoveStep::new` do.
    - Design 04's Numbers gains the rule, beside the refusal of a negative fixed time.
    - Tests: a flaw for a negative value and for a negative rank; an application whose scaling radius and shield are negative holds 0, and its snapshot restores.
    - The change of behaviour: such a package loads no more, and a negative scaling radius or shield acts as 0.
11. **K9: each script's hooks have one owner** ([Structural rules](design/02-engine-core.md#structural-rules), Books). Size S.
    - Design 02 says the hooks a script defines come from what the load read of it. Today `Units::compile` also fills the `ScriptBook` of the server's world, the client and the load build it from `ScriptFacts`, and `ModePackages::books(rate, scripts)` takes it from its caller, so a caller can give another book. Only the lane test checks that the two agree.
    - `ModePackages` builds its `ScriptBook` once, as the load reads the scripts, and holds it. `books(rate)` takes no book, the load check reads the one it holds, and the match installs a copy as its resource. `Units::compile` only compiles, and `compile_scripts` asserts that the host numbers each script at its place.
    - Test: the lane test's comparison goes, as there is one book.
12. **K10: a command's body found by its address**. Size S.
    - `TickInputs::push` finds each command's body among the payloads by its pointer's address, which holds only while postcard borrows the body from the payload. `Command::read` gives each command its place, from the lengths of what is left to read. Test: the tick inputs test, with an empty body.
13. **V2: the local match pins the round trip to its link model** ([Testing](design/02-engine-core.md#testing-and-diagnostics)). Size S.
    - Today `DelayLine::pin_round_trip` sets the measured round trip and jitter to zero, and `LocalMatch::client` puts the model's round trip into `SyncConfig::jitter_margin`. So a field named for jitter holds a round trip, and the client's estimate of the server's tick lags the server by the downlink.
    - Lightyear 0.30.1 keeps the client at the remote estimate + rtt / 2 + jitter × `jitter_multiple` + `jitter_margin` ticks + 1 + `error_margin` ticks (`lightyear_sync`, `timeline/input.rs`, `sync_objective`), and the remote estimate is the last tick it received + rtt / 2 (`timeline/remote.rs`). Pin rtt to the model's worst round trip, 2 × (`delay` + `jitter`) steps, and jitter to 0, under the default `SyncConfig` (`jitter_margin` 1, `error_margin` 1): the lead is then rtt + 3 ticks, which is the lead today, rtt pinned to 0 with `jitter_margin` = rtt + `error_margin`. `TickDelta` keeps fractions, so an odd round trip halves exactly.
    - The worst round trip, not the mean that Lightyear measures, because the model's jitter is bounded: a lead that covers it never lets an input arrive late, which the scenarios assume.
    - `pin_round_trip` writes the model's values, and `LocalMatch::client` takes `SyncConfig::default()`. The input lead stays, so the scenarios' expected ticks should not change. The remote estimate moves half a round trip later, to the server's tick; where an expected value changes, the step derives it again and names it.
    - Joins: when every scenario's join order is now fixed, the scenarios name each client's team, and `play_by_team` goes.
    - Tests: the lead after the sync, read from Lightyear's timelines, is rtt + 3 ticks under `PERFECT` and `DELAYED`, at 20 Hz and 30 Hz.
14. **V3: one way to read script failures**. Size S.
    - Today three tests read failures their own way: the abilities table of `fn` pointers, as `FailureKind::Raised` drops what the script raised; mode's `failures()`, which maps them to `Option<ApiError>`; and orders' `think()`, which asserts an empty list.
    - `FailureKind::Raised` holds the `NumError` a script raised, `None` for another value. The three read `ScriptFailures::calls` and compare with `assert_eq!`.
    - Tests: the abilities table's overflow case expects `Raised(Some(NumError::Overflow))`, and a raise of another value expects `Raised(None)`.
15. **V4: one hero at two rates**. Size S.
    - Today the reference heroes run at 30 Hz in `reference_abilities` and at 20 Hz in the 3v3, and no test compares an action's timing across rates.
    - `reference_abilities` casts Cinder's Eruption at 20 Hz and at 30 Hz. Its area's `delay_ms = 625` is ⌈625 / 50⌉ = 13 ticks at 20 Hz and ⌈625 × 30 / 1000⌉ = ⌈18.75⌉ = 19 ticks at 30 Hz, from its landing to its effect.
16. **F3: the handles of new deliveries**. After unit script state. Changes behaviour. Size M.
    - Today `ctx.projectile` and `ctx.area` take their unit's id only later, so they return `()`, and Cinder's `chain_fire.rhai` cannot use the result.
    - A unit that a call creates takes its id when it is queued, as `spawn_unit` does. Calls run in a stable order, so the ids stay in a stable order. `ctx.projectile` and `ctx.area` return a `Unit` handle that the same call can use, and whose `.state` it writes.
    - It waits for unit script state, a unit's `[state]` and `unit.state`, which the API does not have yet: a handle alone gives a script nothing to use.
