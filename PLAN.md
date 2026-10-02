# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

The structural redesign comes first, in the order below, as [Structural rules](design/02-engine-core.md#structural-rules) asks; each step's shape moves into the design when the step lands. Each step ends with the check chain, both goldens and the structure tests that exist then; a step that changes behaviour names the change. The game model's own steps sit where the redesign makes room for them.

The review of the redesign's diff found two defects and two weak shapes; K1 closes what is left of them before F3. A second review found four more defects, a fact with two owners and a small flaw; K7 closes what is left of them, also before F3.

1. **K1: every way that applies a modifier gives its params** ([Stats](design/04-capabilities/stats.md#modifiers), Appliers). Changes behaviour. Size M.
   - Today the load accepts a param when one applier declares it, and an application that does not resolve returns with no error: `ctx.add_modifier(target, "slow", 1000)` in the lancer's `quake.rhai` loads and does nothing. `has_modifier` counts as an application.
   - The registry marks a modifier argument by its way: `add_modifier` applies with the call's action, `add_player_modifier` with none, and `has_modifier` only names the modifier. The existence check takes all three.
   - `ModifierWays`, in its own file in the package crate, finds each modifier's ways from data and from the scripts' literal names, with auras and modifier scripts passing on the ways of their modifier until nothing changes. It replaces `Package::appliers`, and `LoadCheck::modifiers` and `ModePackages::stat_graph` read it. Each refusal is a `LoadProblem` that names the modifier, the param and the way.
   - A modifier's entry keeps the applier params its numbers read and the ranks its own per-rank params hold. `ctx.add_modifier` and `ctx.add_player_modifier` fail the call when the call's way lacks one.
   - A scaling param's sum stops at the end of the number range. With every way checked, `ModifierBook::application` cannot fail, and the three returns that drop an application go. `CallError::ParamOverflow` goes when nothing raises it.
   - Tests: a flaw for each refusal (an action that lacks the param, a way with no action, an aura's and a modifier script's ways passed on, a short own per-rank param, a time that reads a scaling param, `has_modifier` as no way); a call with a computed name; the saturated sum's exact values.
   - The change of behaviour: a cast whose scaling param leaves the number range plays with the range's end, where the call failed. Reference content that breaks the rule is fixed in the step, and named.
2. **K7: a restore refuses a state a system panics on** ([Structural rules](design/02-engine-core.md#structural-rules)). Size M.
   - Today three flaws restore and then panic or act: a `Route` that waits for an answer with no goal panics in `plan_routes`; a position within the world's bound but outside the map's panics in vision, as no check reads the map's bounds; and a modifier's clock whose interval or shield its modifier does not have reaches the `debug_assert` of `ModifierClocks::renew`, or fires an interval the modifier lacks. The snapshot fuzz flips one byte at a time, so it makes none of them.
   - `Route` holds the tick it asked in with its goal, `goal: Option<Goal { at, asked }>`, so an ask with no goal cannot be expressed.
   - The state registry takes a check of a state type from a capability that does not own it: `Units` checks each position against the map's bounds, which the sim crate does not know.
   - `ModifierClocks::check` compares each clock with its modifier's spec: an interval and a shield exactly when the spec has one.
   - Tests: a restore test of each flaw above; and a structured fuzz beside the byte flips, which writes into a proving match's snapshot, for each state type, values that pass its decode, restores it, and plays five ticks on what restores.
   - Design 02's rule of restored state names the structured fuzz among what enforces it.
3. **F3: the handles of new deliveries**. After unit script state. Changes behaviour. Size M.
   - Today `ctx.projectile` and `ctx.area` take their unit's id only later, so they return `()`, and Cinder's `chain_fire.rhai` cannot use the result.
   - A unit that a call creates takes its id when it is queued, as `spawn_unit` does. Calls run in a stable order, so the ids stay in a stable order. `ctx.projectile` and `ctx.area` return a `Unit` handle that the same call can use, and whose `.state` it writes.
   - It waits for unit script state, a unit's `[state]` and `unit.state`, which the API does not have yet: a handle alone gives a script nothing to use.
