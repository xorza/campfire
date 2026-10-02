# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

The structural redesign comes first, in the order below, as [Structural rules](design/02-engine-core.md#structural-rules) asks; `.notes/REDESIGN.md` holds each step's shape until the step lands and its part moves into the design. Each step ends with the check chain, both goldens and the structure tests that exist then; a step that changes behaviour names the change. The game model's own steps sit where the redesign makes room for them.

The review of the redesign's diff found two defects and two weak shapes; K1 to K4 close them before F3.

1. **K1: every way that applies a modifier gives its params** ([Stats](design/04-capabilities/stats.md#modifiers), Appliers). Changes behaviour. Size M.
   - Today the load accepts a param when one applier declares it, and an application that does not resolve returns with no error: `ctx.add_modifier(target, "slow", 1000)` in the lancer's `quake.rhai` loads and does nothing. `has_modifier` counts as an application.
   - The registry marks a modifier argument by its way: `add_modifier` applies with the call's action, `add_player_modifier` with none, and `has_modifier` only names the modifier. The existence check takes all three.
   - `ModifierWays`, in its own file in the package crate, finds each modifier's ways from data and from the scripts' literal names, with auras and modifier scripts passing on the ways of their modifier until nothing changes. It replaces `Package::appliers`, and `LoadCheck::modifiers` and `ModePackages::stat_graph` read it. Each refusal is a `LoadProblem` that names the modifier, the param and the way.
   - A modifier's entry keeps the applier params its numbers read and the ranks its own per-rank params hold. `ctx.add_modifier` and `ctx.add_player_modifier` fail the call when the call's way lacks one.
   - A scaling param's sum stops at the end of the number range. With every way checked, `ModifierBook::application` cannot fail, and the three returns that drop an application go. `CallError::ParamOverflow` goes when nothing raises it.
   - Tests: a flaw for each refusal (an action that lacks the param, a way with no action, an aura's and a modifier script's ways passed on, a short own per-rank param, a time that reads a scaling param, `has_modifier` as no way); a call with a computed name; the saturated sum's exact values.
   - The change of behaviour: a cast whose scaling param leaves the number range plays with the range's end, where the call failed. Reference content that breaks the rule is fixed in the step, and named.
2. **K2: restored times and counts stay within what a match makes** ([Structural rules](design/02-engine-core.md#structural-rules)). Size M.
   - Today a restore accepts values that panic or hang later: an interval of `u64::MAX` ticks overflows in `ModifierClocks::advance_intervals`; a `stack_life` of `u64::MAX` overflows in `Modifiers::set_stacks`; a repeating timer of 0 ticks loops in `run_timers`, and one of `u64::MAX` overflows in `Timers::fire`; a `next_seq` or `IdAllocator` at `u64::MAX` overflows at its next use; an attack under way that resolves sooner than its windup underflows in `strike`.
   - One limit, `Tick::LIMIT`, 2⁶². Every sum and difference a system takes of a restored value is listed, and the check of its type refuses what breaks it: the tick, times and counts above the limit, a period of 0, and each relation such as the attack's.
   - Tests: a restore test per state type at its limits: the value at the limit restores and plays a tick, and the value past it is refused; the cases above among them.
3. **K3: a book error names its place by type**. Size S.
   - Today `BookError` holds its names as strings, and `LoadCheck::book_error` parses them again and takes a unit type for the avatar when its name is the package's: an avatar package `x` with a delivery type `x` whose area time does not count gets the avatar's place.
   - `BookError` holds a `DeclaredName` for an action, a modifier and a delivery type, and a unit type that stands as the package's avatar or a declared name. `LoadCheck::book_error` maps each to its `Place` with no comparison and no parse.
   - Test: the case above names the delivery type.
4. **K4: each effect type applies itself** ([Scripting](design/02-engine-core.md#scripting)). Size S.
   - Today an effect type's `CAPABILITY`, its row in `CAPABILITIES` and the type its row's `apply_next` takes are three facts that must agree, and a mismatch panics at the first effect.
   - `Effect` loses `CAPABILITY` and gains `apply(self, world, frame, now)`. `Effects` finds a type's queue by its `TypeId`, as `CallParts` finds a part, and records with each effect the apply of its type. The table's effect column, `DISPATCH`, `Frame::set_dispatch` and the seven `apply_next` go.
   - Test: the effects test with two types of one capability.
5. **F3**: the handles of new deliveries, which waits for unit script state.
