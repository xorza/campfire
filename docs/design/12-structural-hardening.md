# Structural hardening

A proposal for one slice of refactoring: it closes 31 of the issue log's 34 open items, the shader test and the coverage lane among them only as [your decisions](#your-decisions) allow. Most items are symptoms of six shared causes, and the fix of each cause makes its whole class of defect fail to compile, fail a test, or panic in a debug build. The facts come from four read-only audits of the code at commit `83ba0733c`, each claim checked against the code (2026-10-20). When the slice ends, each lasting decision moves into the document of the system it governs, the plan moves to `PLAN.md` as it starts, and this document is deleted ([A slice's proposal is temporary](../../AGENTS.md#documents)).

## Causes

- **C1. A contract that the code keeps by convention, not by its types.**
  - A book is a mutable `Resource`, so "an immutable book" holds only while each helper remembers `load_into`'s check: about 32 test sites in 18 files write books in place (**Release**).
  - `Establish` gives the capability "routes by connection ID" as a `bool` beside a `challenge` method that each implementor must write, so a plain and a sealed client hold unreachable methods (**Net-packets**).
  - `MapBook` holds `metric` and `ground` as two independent fields, so a spatial map holds a ground that no code reads (**Mode**, spatial ground).
  - The death record `Fallen` keeps no state of the victim, so each reward pass reads the live world again and cannot see what the victim was at death (**Mode**, bounty for a site; the same gap in the experience share).
  - The in-process predicting client spawns units without what replication brings, and a strike that misses its query is silent, so a windup stays due forever with no assert (**Combat**: a networked client receives `ClipShots`, as its kind is sent and predicted, so the log's item holds for the in-process client alone).
- **C2. "A pass visits what changed" has blind spots.**
  - Change ticks age on a unit that `Unpredicted` hides, and each pass must remember `Removals<Unpredicted>` by hand: 2 of the 5 client passes do (**Stats**).
  - `route_units` and five more passes read every entity's change ticks each tick, outside the measurement that 00-overview's budget of change scans rests on (**Navigation**).
  - The endpoint does not record the deadline its IO thread sleeps to, so each frame assumes it is stale and wakes the thread (**Net-endpoint**).
- **C3. A rule holds the cases it lists, not their class.**
  - The parallel ban lists `par_iter*` methods, so `ParallelCommands` is outside it (**Net-messages**).
  - The resource audit reads sim worlds only, so a resource that no system of an app reads goes unseen (**Net**, `OutView`).
  - 02's "A type name that both sides declare keeps one meaning" has no test and no row in its Structural rules: about 160 names are declared in two or more crates, and more than 30 have two meanings, of which the log holds 4 (**Protocol**, **Stats**, **Store**, **Import**). No test reads feature names either (**Net-stack**, the `sim` feature).
  - `citations` reads a title from a bullet's first line, so a wrapped title is skipped, not reported (**Rules**).
- **C4. A replay starts only at tick 0.** A log has no file form that starts at a checkpoint, so no verifier verifies a segment from its checkpoint (**Verifier**, `LogCarry`); each test that asks whether a match replays to the same hashes plays the whole match again (**Runner**, the 3v3 scripted match and its resume).
- **C5. Engine data can only be read.**
  - Sections are deserialize-only, read through seeds and `dyn` kind sections, and an overlay needs a partial section that the engine has no type for, so the importer mirrors every key it writes (**Import**).
  - A name's kind lives only on a single returned `IdName`, so lists of names and hook arguments are plain text that the load checks against nothing (**Script**).
  - The rule "a press is a click or a drag" is split between code (the pixels), `camera.toml` (the time) and the import (which reads the time alone) (**Client**, drag tolerance).
- **C6. The test budget breaks for causes outside the code under test.**
  - The proving sweeps run one case after another on one thread (**Runner**, three tests and an unlogged fourth).
  - Two tests keep their files on btrfs, where fsync latency comes from other writers (**Verifier**, timeouts).
  - A 5 s stop leaves no margin over the 1 s bound under a full run's 2.5× slowdown, and no record names a test that timed out.

## Decisions

### Contracts as types

- **A book is an immutable resource.** `Book` requires `Resource<Mutability = Immutable>`, so each book declares `#[component(immutable)]` or fails to compile, and `resource_mut` or `ResMut` on a book fails to compile (bevy_ecs 0.20, `World::resource_mut` takes `Mutable` alone); a test changes a book before its match installs, through `BookLoad::amend`, which makes `load_into`'s checks and inserts the book again. Refused: an assert in each helper, which the next helper forgets; a run condition for each fact, which 02 refuses (A test's match installs at its first tick).
- **A path check is a server's capability.** `ServerEstablish::path_check() -> Option<&mut dyn PathCheck>`, where `Some` both routes by connection ID and challenges a new path, as only the server routes so (11-net-stack/udp.md, A client packet starts with the server's random connection ID); `routes_by_prefix` and `challenge` leave `Establish`. Refused: a default method that panics, which is still unreachable; an endpoint generic over its establishment, which packet-session.md refuses.
- **A spatial map has no sim ground.** `MapBook` holds `MapSpace::Planar(Ground) | Spatial`, its metric derived from it, and the load refuses `[ground]` on a spatial map; the grid stays the client's terrain. Refused: a load rule alone, which leaves a struct that can hold a ground no code reads.
- **A death records its victim.** `Fallen::of(EntityRef)` is the one constructor and keeps the victim's tags at death; `[bounty]` and the experience share take a `victim` tag filter, all by default, and the Zero Hour import writes `!constructing`, the tag a site holds, as `Player::doBountyForKill` and `Object::scoreTheKill` skip a victim under construction. Refused: `Has<Site>` in `Bounty::pay`, which names production's part in `mode` and one game's rule in a capability ([A capability serves every game](../../AGENTS.md#direction)); the victim's live tags, which a death rewrites and a despawn takes.
- **The test client holds what a networked client holds.** `Match::predicting` spawns each unit with what its client receives, through `ClientHolds`' kind rules, and one `Striker` query serves `Volley::collect` and the strike, whose miss debug-asserts that the attacker died or went. Refused: registering a server part on the client, which gives an unpredicted unit a part it never receives.

### A pass visits what changed

- **Prediction's start marks the unit's state changed.** An observer of `PredictionStarted` sets each registered mutable state part of the unit changed, through `StateRegistry`, so every pass that follows change ticks takes the unit, and the passes drop their own `Removals<Unpredicted>` reads; the due indexes keep their push (00-overview, One due index for each timed process). Bevy gives no signal as an entity is enabled again, and its entity disabling leaves change ticks as they were: who enables an entity writes the change. Refused: the read in each pass, which 3 of 5 forgot; `Allow<Unpredicted>` on those passes, which writes units the client holds as sent.
- **Every change scan is measured against one budget.** `route_units` and the five unmeasured scans join the change-scans bench, so 00-overview's budget, at most 5 % of the 3v3's mean tick for all of them, covers every pass that follows change ticks. A pass gets an index of its own only when the total passes the budget, as 00-overview refuses a mark from each writer below it. Refused: an index for `route_units` now, which 00-overview refuses for the same saving; `Changed<>` filters, which read every entity's ticks too.
- **The IO thread wakes only for a deadline it does not know.** The runtime records, under its lock, the deadline it sleeps to, and one `EndpointRuntime::step` runs the frame's work on the core, sends the datagrams it made, and wakes the thread only when the core's next deadline is earlier than the one recorded, as quinn's connection driver moves its timer only when `poll_timeout` changes. Refused: a flag set when the frame took a payload, which wakes for datagrams the frame already sent; the send queue's state, which the frame's own send empties.

### Rules that hold the class

- **No parallel type outside its decision.** `clippy.toml` bans `ParallelCommands` and `bevy_utils::Parallel` beside `par_iter*`, with the same reason (02-engine-core.md, No query runs in parallel), and the message receive path takes `&mut Commands`.
- **An app's resource is one that a system reads.** A net-harness check walks the access of each schedule's systems once a client joins, through Bevy's own `component_access_set`, and lists each resource of the world that none reads, past an allowlist of host reads with a reason each; `OutView`'s resource goes, with `visibility`'s `ecs` feature that it alone needed. Refused: `unlisted_resources` on an app world, which audits the sim's declarations; a lint, which cannot see a resource inserted at run time.
- **A type name has one meaning in the workspace.** A `rules` test reads every crate with `syn`, maps each type, trait and alias name to the crates that declare it, and fails on a name in two crates that `ONE_MEANING` does not list, and on an entry no longer in two crates; each entry cites the Names decision that gives its one meaning, so the citation test holds the list to Names. A second test refuses a feature of the stack named after a crate of Campfire's. Each collision takes the qualified name of its narrower use, as 02 decides. Refused: `pub` items alone, as `pub(crate)` names still clash in one workspace; today's collisions in the list, which keeps one name of two meanings; Rust's module-qualified names, as `io::Result` beside `fmt::Result`, which 02 refuses for this workspace, whose names a grep and a review read bare.
- **A structural rule has its row.** 02's Structural rules table gains a row for each rule this slice adds a test for: one meaning per type name, immutable books, the parallel types, and an app's resources.
- **A citation's title is its bold lead, whole.** `titles` joins a bullet's lines to its first blank line, bullet or heading, reads nested bullets, and reports a lead that never closes. Refused: a Markdown crate, a new dependency.

### A replay starts at a checkpoint

- **A log file may start at a checkpoint.** The file holds a base, its segment and that segment's signed checkpoint record, whose layout 05 gains under a new log tag, `campfire/session-log/v7`; one function builds a log from a carry, for the file's decode and for a load. A verifier of a based log checks the base's snapshot against the record's signed snapshot fingerprint and state hash before it replays, and refuses a based log with no snapshot, so it trusts only what the server signed, as Ethereum's checkpoint sync starts from a signed state. `Replay::from_checkpoint` restores the checkpoint through `Session::resume` and plays the segment. Refused: a server's journal for verifiers, whose state a verifier must not trust; replays from tick 0 alone, which cannot prove a converted save's segment (02-engine-core.md, One-way converters).
- **A match's replay test replays its segments.** The 3v3 scripted match checkpoints every 1,000 ticks, as a server checkpoints by its plan, and its replay check replays each segment from its checkpoint through `threads::Parallel`; the resume test goes, its check held by the scripted match at 15 checkpoints. Each checkpoint starts a segment with a new seed, so the match's draws change once: its goldens move, and each of its hand-derived numbers that a draw sets is derived again. Refused: a restore path for tests alone, which the verifier never runs; fewer ticks, as the match needs level 6; a match of no checkpoint, which no server plays.

### Engine data written from its own types

- **The manifest writes as it reads.** `ModeManifest` has one serde shape for both directions, through its fields' struct, and the import builds it and writes it through `codec`.
- **A section has a partial form.** The owning module declares `Section::Patch`, the all-`Option` twin of its fields, and the section's read goes through its patch and then checks it, as `TurretData` reads through its fields, so the patch is the one list of its fields; `declarations` writes a patch under its section's key, and the import writes patches, so a renamed field breaks its build. Practice: Kubernetes' apply configurations, all-optional twins made for a partial apply. Refused: sections written whole, which 03 refuses (Overrides by load order); a schema test over the mirrors, which keeps two lists of fields; a derive of the twins, until a third crate needs one.
- **A name's kind reaches lists, hook arguments and variables.** A script output's name kind holds a shape, one or a list, with `IdNames<I>` for `unit.queue`, `ctx.map.paths`, `ctx.teams` and `ctx.enemies`; a hook's arguments take `IdName`; the load checks `contains`, an index, and the variables a `for` or a hook's parameter binds until a `let` shadows them. The check refuses only what it proves wrong, and a name it cannot follow stays unchecked. Refused: typed handles, which 06 refuses.
- **A press is one rule of the mode's data.** `client/keys.toml`'s `press` holds the drag in pixels and the click in seconds, which the selection and the camera both read, and the import writes both from `Mouse.ini`'s `DragTolerance` and `DragToleranceMS`; the time leaves `camera.toml`. Refused: a drag on `CameraFile`, as a selection is not the camera's.

### Tests within their budget

- **A sweep runs on every core.** The proving sweeps of restores, byte flips and mutations run their cases through `threads::Parallel`, each worker on its own build of the shared packages, in a module that nextest gives `threads-required = "num-cpus"`, as the swarm has it (11-net-stack/testing.md, A test runs in 1 s or less). Refused: fewer draws, which trades coverage for the budget; a table test for each case, which builds the fixture again in each process.
- **A test's files are on the tests' file system.** Every test makes its directory with `TestDir::new()`, and `new_in` goes; nextest writes a JUnit report, so a timeout names its test. Refused: a longer stop, which hides the cause.

### Local fixes, no new decision

`DebandDither::Disabled` beside `Tonemapping::None`; the 17 `[lib] bench = false` go; the LAN check keeps its newest runs; the endpoint bench, the join burst on a rate-limited link, and the shader composed in a test measure what no test measures now.

## Plan

Each batch is one commit, ends with `cargo drive gate` passing, and stops for review. The batches go by impact: first what makes every later gate reliable and every later rule hold its class, then the contracts, then the larger moves. A batch never goes before one it needs.

### 1. Tests within their budget (S)

`harness`, `verifier`, `.config/nextest.toml`, the 17 manifests. First, as every later gate runs these tests.

- Run the proving sweeps of `harness/tests/runner/proving_match.rs` (drawn values, byte flips, mutations) through `threads::Parallel` in a `sweep` module, with its override in both nextest profiles.
- `TestDir::new()` in `verifier/tests/binary.rs:26` and `harness/tests/verifier.rs:227`; delete `TestDir::new_in`; add `[profile.default.junit]`.
- Delete each `[lib] bench = false`.
- **Check:** each sweep within 1 s alone, with the same counts of restored and refused cases; the suite within 30 s; 20 full runs with no timeout, read from the JUnit reports.

### 2. Rules that hold the class (S)

`rules`, `clippy.toml`, `net-messages`, `net`, `visibility`, `net-harness`, `02-engine-core.md`. Before the names and the contracts, so their new code meets the new rules.

- Ban `ParallelCommands` and `Parallel`; take `&mut Commands` through `MessageReceiveCx`, the receivers and the remote events.
- Whole-bullet titles in `rules/src/citations.rs`.
- Delete the `OutView` resource and `visibility`'s `ecs` feature; add the app-world resource check to the net harness.
- 02's Structural rules gain their rows.
- **Check:** each new test fails on its sample of a breach and passes the workspace; no golden moves.

### 3. One meaning for each type name (M)

`rules`, `02-engine-core.md`, and each crate whose names change.

- The type-name test and the feature-name test.
- The renames, each to its narrower use: the feature `sim` → `emulator`; `protocol`'s `Packet` → `PlayerPacket`; `stats`' `Clock` → `ModifierClock`; `store`'s `Slot` → `Latest`, `script`'s `Slot` → `TypedItem`; `import`'s `MapFile` and `UnitsFile` → Zero Hour's own names; and the rest the audit lists (`Start`, `Started`, `Channels`, `PublishError`, `SeatStart`, `Taken`, `Spent`, `Raised`, `Modifiers`, `DropCause`, `Named`, `Side`, `Header`, `Outcome`, `Reader`, `Pending`, `Run`, `Server` and others).
- `ONE_MEANING` lists the shared conventions, as `CommandLine`, `Args` and `*Fields`, each citing its Names decision, and Names gains one for each convention it lacks.
- **Check:** the test fails on a sample name in two crates and on a stale entry; no golden moves.

### 4. Contracts as types (M)

`declarations`, each crate with a book, `net-packets`, `net-udp`, `units`, `mode`, `progression`, `combat`, `capabilities`, `import`, `client`.

- Books immutable, with `BookLoad::amend`; each of the 32 sites changes its book before its match installs, or builds a new match.
- `PathCheck` on `ServerEstablish`; delete the unreachable `challenge`s and `routes_by_prefix`.
- `MapSpace` in `MapBook`, and its load error.
- `Fallen::of`, and the `victim` filter of the bounty and the experience share; the import writes `!constructing`.
- `Match::predicting` spawns as a client receives; one `Striker` query; delete the hand insert at `capabilities/tests/combat/mod.rs:237`.
- `DebandDither::Disabled` at `client/src/view/player_camera.rs:77`.
- **Check:** `resource_mut` on a book does not compile, and `amend` after install panics; a plain server that meets a new address opens a new peer and challenges no one; a spatial map with `[ground]` fails its load; a site killed under construction pays no bounty and gives no experience with the filter, and both without it; the clip test passes with no hand insert. The Zero Hour golden moves only if its check kills a site.

### 5. A pass visits what changed (M)

`sim`, `stats`, `items`, `actions`, `navigation`, `net-endpoint`, `net-packets`, `net-socket`, `research/change-scans.md`.

- The observer of `PredictionStarted`; delete the passes' `Removals<Unpredicted>` reads.
- `route_units` and the five unmeasured scans join the change-scans bench, and the research records the total.
- The recorded deadline and `EndpointRuntime::step`.
- **Check:** a while-ready passive whose cooldown ends while its unit is hidden is held one tick after prediction starts, which fails today, and a rollback test makes no extra rollback; the scans' total within 5 % of the 3v3's mean tick, or an index for the pass that passes it, in a decision of its own; an idle linked endpoint wakes its thread 0 times in 100 frames, and a send that arms an earlier timer wakes it once; the UDP and emulated scenarios pass.

### 6. A replay starts at a checkpoint (L)

`protocol`, `05-protocol-spec.md`, `runner`, `verifier`, `harness`, `threads`.

- The based log file under `campfire/session-log/v7`, its layout in 05; one build of a log from a carry.
- `Replay::from_checkpoint`; the verifier checks a base's snapshot against its signed record and verifies a based log.
- The 3v3 scripted match checkpoints every 1,000 ticks, on a seed chain long enough for its 15 segments, and replays its segments in parallel; delete the resume test.
- **Check:** a 3v3 log cut at each checkpoint verifies to the whole log's final hash; a changed carry fails its signature; a snapshot whose fingerprint differs from its record is refused, as is a based log with no snapshot; the scripted match within its 15 s alone. Its goldens and draw-set numbers move once, for the new segments' seeds, and the batch names that cause.

### 7. Engine data written from its own types (L)

`package`, `declarations`, each section's crate, `import`, `script`, `client-files`, `client`.

- The manifest's serde shape both ways; the import writes it.
- `Section::Patch` for the sections the import writes, and the table writer; the import's mirrors go.
- Name kinds on lists, hook arguments and bound variables.
- `press` in `keys.toml`; `MouseIni` reads `DragTolerance`.
- **Check:** each patch reads back through its section's seed; the Zero Hour import's package loads; a misspelled `on_timer` name and a misspelled queued type fail the load, naming the kind, and a name the check cannot follow loads; the selection drags past the mode's pixels.

### 8. Measurements (M)

`net-packets`, `net-harness`, `client`, `lan-check`.

- The endpoint bench: 64 to 4,096 links and a flood of handshakes, the mean and worst hold of the lock; a bound on new links in a pass only as the measurement shows.
- The join burst on a rate-limited link, its largest frame and its time to sync pinned as exact bytes.
- The LAN check keeps its newest runs; its late input is researched from the CI run's bot logs before any fix.
- The shader composed in a test, if naga becomes a dev-dependency.

## Your decisions

Each one waits for your answer before its batch starts.

- **The LAN check's runs:** keep the newest 3 (recommended, as pytest's `tmp_path_retention_count`), or each failed run and the newest.
- **World:** keep multithreading over regions as a planned exception that 02 names (recommended, as 01's world of 1,000 players rests on it), or remove it.
- **naga as a dev-dependency** of `client`, for the shader test; it is already in the lockfile through Bevy.
- **The coverage lane:** an exception in 02's driver for its own `RUSTFLAGS` and target directory, or no lane.

## Left open

Three items stay in the log: the LAN check's late input (**Net**), which needs the CI run's logs first; and the endpoint pass and the join burst (**Net-stack**), whose fixes depend on what batch 8 measures.

## Checks

- Each batch ends with `cargo drive gate` passing, and a golden that moves names its batch and its cause.
- `cargo drive test` runs with no test past its bound and the suite within 30 s, the scripted match within 15 s alone.
- Each new rule test fails on a sample of the breach it holds.
- The issue log loses each item its batch closes, as the batch closes it.
