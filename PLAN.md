# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

The structural redesign comes first, in the order below, as [Structural rules](design/02-engine-core.md#structural-rules) asks; `.notes/REDESIGN.md` holds each step's shape until the step lands and its part moves into the design. Each step ends with the check chain, both goldens and the structure tests that exist then; a step that changes behaviour names the change. The game model's own steps sit where the redesign makes room for them.

1. **A2 Proving match**: a mode in `packages/test` that uses every capability the release runs, with scripted bots: combat, stats, abilities with projectiles and areas, production with two producers of one player, progression, navigation with changing static bodies, vision with several groups.
2. **A3 Goldens**: `HashTrail`, the behaviour trace, and the state and behaviour goldens over the lane match, the proving match and the 3v3; the first work record.
3. **A4 Capability table and layer test**: one table of capabilities (install order, needs, layer), the layer test over each module's imports with today's breaks as exceptions.
4. **B1 Unit life**: `die` stops actions; the life pool needs `[combat]`; delivery types refused for spawns; `/` refused in package names and ids.
5. **B2 Order and timing**: `Ordered` and its users; the archetype-shuffle test; `finish_trains` at the tick's end; Mode-stage events defer as timers do; the projectile type decides homing; `renew` takes the new interval.
6. **B3 Navigation stalls**: `nearest_open` within the window; the route kept on an empty short plan; ask ticks kept; the AI's attack order through `weapon_for`.
7. **B4 Inputs and lobby**: inputs counted per stamp and applied with a limit per tick, the rest spilled to later ticks (design 05); the client keeps the limits; sizes checked before hashing; lobby disconnects; one seat per key; the server's exit code.
8. **D4 Effect dispatch**: typed effect queues per capability and the dispatch table; `CallStart` with the call's package in the frame.
9. **Effects in data**: **Effects in data**: effect lists `on_resolve`, `on_hit` and `on_end` before their hooks; `d.hit`; `calc_heal`. Test: Rime's Fan of Frost from data alone, hitting exactly the units hand-placed in reach; a heal that `calc_heal` halves.
10. **C1 Packages read once**: a package read once into memory and fingerprinted over those bytes; one `/` grammar for package paths; per-package failures in the store; the verifier checks the release first.
11. **C2 Package content and API version**: `PackageContent` for every package kind; `PackageIndex`; unit type identity by package and name, with its scopes. And the package API version: **Package API version**: the manifest's `api` in place of `engine`, the registry's version of each name, the rule of major and minor at load, the version in the reference. Test: a package of an older minor loads; one of another major, and one of a newer minor, fail with their errors.
12. **C3 Checked names**: names as checked types where they enter; tag names with reserved engine tags; mode and modifier state types; stat values as checked numbers.
13. **Human text**: **Human text**: `locale/<language>.ftl`, message ids in data and scripts, the load check that every id has its text, the `locale` package kind; the reference packages' text moved to `en.ftl`. Test: a hero's name in a second language from a `locale` package; a missing id refused at load; a match whose hashes do not change with the language.
14. **C4 Name arguments from the registry**: argument roles in the registry; generic script facts; hook arity from the registered signature.
15. **C5a Books built by the load**: `Books::build` beside the load check, results compared; times checked at the fastest rate.
16. **C5b Load is the check**: `ModePackages` holds the books, `Match::install` reads them, `StartError` keeps only session terms, each script parsed once.
17. **C6a Typed ids**: one list for each id (`StatId` and the others); one ms-to-ticks conversion.
18. **C6b Modifier specs**: the modifier runtime spec; param tables in the books; stats no longer reads the frame.
19. **C7 Shared books**: the view and the frame read the books through `Arc`; `ScriptConsts`; `MatchScripts` goes; the name-lookup allowlist test.
20. **C8 Action kinds and type roles**: `ActionKind` with its data; `Delivery` with its unit type; `TypeRole`; an attack's damage names its weapon.
21. **C9 Arena and fixed session**: `Arena` loads the real packages through `Match::install`; `FixedSession`.
22. **I1 to I4 Network session**: `Tick` in `math`; `SessionRules`; `JoinState`; `Prediction::install`.
23. **Client stats and tags**: **Client stats and tags**: one unit type order that the server and the client read from the packages; the client builds the stat book, the server sends each unit's type once and its level, and the step is derived on both sides, no longer sent; the client holds the mode's param tables, so it computes a live stat change as the server does; the server sends the relations as a script changes them, so the client's targets and filters agree with the server's; the client starts its own units' actions through the core's checks, as the server does. Test: a client whose unit is slowed by a modifier walks in step with the server; a stun that ends on the server ends on the client in the same tick, with no correction; a client's hero starts its attack in the tick the server does.
24. **D2, D3, D5, D6 Layers**: the action pipeline below combat; shots and the spawner; view columns; owners of runtime values. Test: the layer test has no exception left.
25. **E, F2, G, H Remaining parts**: one reach rule; exactness; checked restore and state without book data; work limits and shared indexes, in the order `.notes/REDESIGN.md` gives.
