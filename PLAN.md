# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

The structural redesign comes first, in the order below, as [Structural rules](design/02-engine-core.md#structural-rules) asks; `.notes/REDESIGN.md` holds each step's shape until the step lands and its part moves into the design. Each step ends with the check chain, both goldens and the structure tests that exist then; a step that changes behaviour names the change. The game model's own steps sit where the redesign makes room for them.

1. **C2 Package content and API version**: `PackageContent` for every package kind; `PackageIndex`; unit type identity by package and name, with its scopes. The package API version: the manifest's `api` in place of `engine`, the registry's version of each name, the rule of major and minor at load, the version in the reference. Test: a package of an older minor loads; one of another major, and one of a newer minor, fail with their errors.
2. **C3 Checked names**: names as checked types where they enter; tag names with reserved engine tags; mode and modifier state types; stat values as checked numbers.
3. **Human text**: `locale/<language>.ftl`, message ids in data and scripts, the load check that every id has its text, the `locale` package kind; the reference packages' text moved to `en.ftl`. Test: a hero's name in a second language from a `locale` package; a missing id refused at load; a match whose hashes do not change with the language.
4. **C4 Name arguments from the registry**: argument roles in the registry; generic script facts; hook arity from the registered signature.
5. **C5a Books built by the load**: `Books::build` beside the load check, results compared; times checked at the fastest rate.
6. **C5b Load is the check**: `ModePackages` holds the books, `Match::install` reads them, `StartError` keeps only session terms, each script parsed once.
7. **C6a Typed ids**: one list for each id (`StatId` and the others); one ms-to-ticks conversion.
8. **C6b Modifier specs**: the modifier runtime spec; param tables in the books; stats no longer reads the frame.
9. **C7 Shared books**: the view and the frame read the books through `Arc`; `ScriptConsts`; `MatchScripts` goes; the name-lookup allowlist test.
10. **C8 Action kinds and type roles**: `ActionKind` with its data; `Delivery` with its unit type; `TypeRole`; an attack's damage names its weapon.
11. **C9 Arena and fixed session**: `Arena` loads the real packages through `Match::install`; `FixedSession`.
12. **I1 to I4 Network session**: `Tick` in `math`; `SessionRules`; `JoinState`; `Prediction::install`.
13. **Client stats and tags**: one unit type order that the server and the client read from the packages; the client builds the stat book, the server sends each unit's type once and its level, and the step is derived on both sides, no longer sent; the client holds the mode's param tables, so it computes a live stat change as the server does; the server sends the relations as a script changes them, so the client's targets and filters agree with the server's; the client starts its own units' actions through the core's checks, as the server does. Test: a client whose unit is slowed by a modifier walks in step with the server; a stun that ends on the server ends on the client in the same tick, with no correction; a client's hero starts its attack in the tick the server does.
14. **D2, D3, D5, D6 Layers**: the action pipeline below combat; shots and the spawner; view columns; owners of runtime values. Test: the layer test has no exception left.
15. **E, F2, G, H Remaining parts**: one reach rule; exactness; checked restore and state without book data; work limits and shared indexes, in the order `.notes/REDESIGN.md` gives.
