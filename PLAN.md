# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

Each step's shape moves into the design when the step lands. Each step ends with the check chain, both goldens and the structure tests ([Structural rules](design/02-engine-core.md#structural-rules)); a step that changes behaviour names the change.

1. **F3: the handles of new deliveries**. Changes behaviour. Size M.
   - Today `ctx.projectile` and `ctx.area` take their unit's id only later, so they return `()`, and Cinder's `chain_fire.rhai` cannot use the result.
   - A unit that a call creates takes its id when it is queued, as `spawn_unit` does. Calls run in a stable order, so the ids stay in a stable order. `ctx.projectile` and `ctx.area` return a `Unit` handle that the same call can use, and whose `.state` it writes.
2. **N2: engine enums** ([Engine enums](design/08-script-api.md#engine-enums)). Changes the script API. Size M.
   - Today `set_relation` and `spawn_group` take `"hostile"` or `"start"` as strings: a typo in a literal is found only when the call runs, as `UnknownRelation` or `UnknownPathEnd`.
   - The registry gets one builder call for an enum: it binds a static module of constants for the members, records the enum and its members, and the reference lists them. `Relation` and `PathEnd` are the two enums, over `Attitude` and `PathEnd`, each with `named(text)`, `==`, `!=` and `to_string`.
   - `set_relation` and `spawn_group` take the types. The registry marks those arguments as enum arguments; the load refuses a string literal given to one, and a `Module::member` path of no registered enum or member, each a `ScriptProblem` that names the script, the call or path, and the enum.
   - No function pointers ([Checks at package load](design/08-script-api.md#checks-at-package-load)): the load refuses an anonymous function or closure and a call of `Fn`, a `ScriptProblem` that names the script. Today a closure loads, and its `.call` fails the load as an unknown member. With no function pointer, the `ctx` convention's case for `call` and `curry` goes.
   - The reference modes pass their markers' `from` param through `PathEnd::named`. No release has shipped, so package API 1.0 changes in place.
   - Tests: a flaw for each refusal (a string literal for an enum argument, an unknown member, an unknown enum module, a closure, an anonymous function that captures nothing, a call of `Fn`), where N1's case of a closure that loads was; a call with a member, and with a member from `named`; `named` of a text that names no member fails the call; the reference lists both enums.
3. **N3: editor definitions** ([Editor definitions](design/08-script-api.md#editor-definitions)). Size S.
   - First, a measure: generate the file once by hand from the engine the registry builds, open the reference scripts with Rhai's VS Code extension, and record what it completes and what it flags: enum members, global functions, signatures, descriptions. The step goes on only if it completes enum members and signatures.
   - `campfire-script` gets a feature that turns on Rhai's `metadata`; only the test that writes the file uses it. Each registration gives its description as the function's doc comment.
   - A test writes `campfire.d.rhai` beside the reference packages, and fails when the checked-in file differs, blessed with `CAMPFIRE_BLESS=1` as the reference is.
   - Tests: the checked-in file is what the registry writes; it holds each enum's members and a registered call's description.
