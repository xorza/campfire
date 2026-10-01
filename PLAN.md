# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 3: Vertical slice, close

1. **Vertical slice playtest, fourth round**: the same 1v1 on LAN, with routes and collision. Units that block each other and body block are new to the feel; write down whether the match reads and plays well. This closes stage 3, or names what is missing.
2. **Collision bench**: on a quiet machine, run `cargo bench -p campfire-capabilities --features bench --bench collision`: 1000 bodies, crowded into 40 m square and spread over 120 m square, contacts found and parted. Check that it fits the Collide stage's share of a tick at 30 Hz, and record the numbers beside the collision design. A timing probe on a busy machine gave 0.14 to 0.42 ms, against 2.1 to 2.5 ms for a check of every pair.

## Stage 4: Game model

Touches: every capability, the registry, the package loader, the client, the reference packages. Design: [The model](design/04-capabilities/00-overview.md#the-model), the capability docs, [Game package](design/03-game-scripting.md#game-package). Each step rewrites the reference packages to the model as it goes, so the 3v3 match keeps running with the same rules; until its step, the code that differs from the design is the code's to fix.

1. **Deliveries as units and effects in data**: areas as units of their section, and `ctx.area`; effect lists `on_resolve`, `on_hit` and `on_end` before their hooks; `d.hit`; `calc_heal`. Test: Rime's Fan of Frost from data alone and Cinder's Eruption, hitting exactly the units hand-placed in reach; a heal that `calc_heal` halves.
2. **Client stats and tags**: one unit type order that the server and the client read from the packages; the client builds the stat book, the server sends each unit's type once and its level, and the step is derived on both sides, no longer sent; the client holds the mode's param tables, so it computes a live stat change as the server does; the server sends the relations as a script changes them, so the client's targets and filters agree with the server's; the client starts its own units' actions through the core's checks, as the server does. Test: a client whose unit is slowed by a modifier walks in step with the server; a stun that ends on the server ends on the client in the same tick, with no correction; a client's hero starts its attack in the tick the server does.
3. **Package API version**: the manifest's `api` in place of `engine`, the registry's version of each name, the rule of major and minor at load, the version in the reference. Test: a package of an older minor loads; one of another major, and one of a newer minor, fail with their errors.
4. **Human text**: `locale/<language>.ftl`, message ids in data and scripts, the load check that every id has its text, the `locale` package kind; the reference packages' text moved to `en.ftl`. Test: a hero's name in a second language from a `locale` package; a missing id refused at load; a match whose hashes do not change with the language.
