# Campfire — Engine Core

## Module map

```mermaid
flowchart TB
    subgraph apps["Apps"]
        client
        server
        verifier
        launcher:::planned
        editor:::planned
    end
    subgraph checks["Checks"]
        lancheck["lan-check"]
    end
    subgraph services["Services"]
        net
        content:::planned
        store
        identity:::planned
        ownership:::planned
        payments:::planned
    end
    subgraph core["Deterministic core"]
        runner
        package
        capabilities
        script
        sim
        protocol
        math
        common
    end
    client --> net
    server --> net
    net --> runner
    verifier --> runner
    lancheck --> verifier
    lancheck --> net
    server --> store
    net --> store
    runner --> package
    runner --> protocol
    package --> capabilities
    capabilities --> script
    capabilities --> sim
    script --> math
    sim --> math
    math --> common
    protocol --> common
    classDef planned stroke-dasharray: 5 5
```

Dashed: planned, as the table below marks it. `log` is left out: the binaries, `net` and `verifier` use it.

Each layer uses the layers below it.

## Modules

| Module | Status | Does |
| --- | --- | --- |
| `common` | built | The vocabulary that crates which do not depend on each other share: the player slot, ticks, segment seed and 32-byte values written as hex, a package's fingerprint, and the binaries' exit statuses |
| `math` | built | Fixed-point numbers, 3D vectors, trig, exact 256-bit products, counter-based RNG |
| `protocol` | built | The open protocol: signatures, session key delegations, the connection's handshake, input chains, the seed chain and the session log, with no IO ([Protocol Spec](05-protocol-spec.md)) |
| `sim` | built | Deterministic state and systems on `bevy_ecs`; no genre code |
| `script` | built | The Rhai host: compiles scripts, and runs each call under its limits; the script API itself is in `capabilities` ([Script API](08-script-api.md)) |
| `capabilities` | built | Mechanisms a mode combines, a module each: `combat`, `navigation`, `orders` and the rest ([Capabilities](04-capabilities/00-overview.md)) |
| `package` | built | Reads a mode's packages and every package it depends on, and runs the load checks of [Script API](08-script-api.md) |
| `runner` | built | Builds a match from checked packages: wires `sim`, the declared capabilities and `script`, feeds inputs; owns `SessionRules`, which builds a session's terms from the packages and checks terms on the server, the client and the verifier |
| `verifier` | built | CLI: replays a session log segment, checks the result |
| `lan-check` | built | The real server and bot clients over WebTransport on `127.0.0.1`, on request ([Testing and diagnostics](#testing-and-diagnostics)) |
| `server` | built | Host config, lifecycle, saves, validation, admin; a headless app, and a library the client runs on a thread for singleplayer |
| `net` | built | Lightyear over QUIC (WebTransport): handshake, replication; internal |
| `log` | built | The binaries' log output: text on standard error, and JSON lines into a file; the events a tool reads back from those lines; the end of a command line clap refuses or answers with its help |
| `store` | built | Durable and secret files, data directories and their locks, and the worker threads that write them, each with its failure ([Storage](#storage)) |
| `launcher` | planned | Small app: fetches, checks and starts the engine release a server or replay names; server browser |
| `client` | built | Bevy app: rendering, input, UI, audio, prediction |
| `content` | planned | Package signatures, pinning, cache, Blossom fetch |
| `editor` | planned | Map and content editors |
| `identity` | planned | Nostr keys, session keys, listings, reputation |
| `ownership` | planned | License checks (optional) |
| `payments` | planned | Pools, hold invoices, wallet budgets (optional, separate repo) |

A module is built when the workspace has its crate, `campfire-<module>` in `source/crates/` or `source/checks/`, and planned when it does not yet. A test fails when this column and the workspace differ.

`sim` is pure: state and inputs in, next state out; no files, packages or signatures.

`lan-check` is a check, not an engine crate: it lives in `source/checks/`, apart from `source/crates/`, and nothing depends on it. Each crate's folder has its package's name, `campfire-` and the module: `source/crates/campfire-math/`, `source/checks/campfire-lan-check/`.

Dependencies: `server` and `client` → `net` → `runner`; `server` and `net` → `store`; `verifier` → `runner` → `package` → `capabilities` → `script`, `sim`; `runner` → `protocol`; `script` and `sim` → `math` → `common`; `protocol` → `common`. `store` depends on no engine crate, and no crate of the deterministic core depends on it. `log` depends on `common` alone, for the `ExitStatus` a command line ends with; the binaries, `net`, `runner` and `verifier` use it, and the tests of `sim` and `capabilities` use its internals. The runner joins `protocol` and the packages, which name a package by the one `Fingerprint` of `common`. A type enters `common` only when two crates that do not depend on each other both name it, and only as a plain value: construction, parsing, display and serde, and no other logic. `common` depends on `serde` and `derive_more` alone. Within `capabilities`, a module imports only from the capabilities below it.

Outside the engine crates: the reference MOBA and bots. The tests of `package` and `runner`, and `lan-check`, use them as test content; nothing else in the engine depends on them. Bots produce inputs like players, so replays never depend on bot code.

## Structural rules

These rules keep the code's structure from drifting. Each has a test that fails when it is broken, because a rule that only a review checks drifts again.

| Rule | Enforced by |
| --- | --- |
| One owner for each fact. A fact from the packages lives in one immutable book; a fact of the match lives in state; a fact of the running call lives in the frame. Nothing else holds a copy. | The state table test; the behaviour golden |
| A name becomes an id where it enters. After the load, no system looks up a name; a script call resolves its name once per call, with no allocation. Every lookup of an id by its name is a method whose name ends in `named`, so each call of one can be found. | The book builder's tests; the allowlist test of name lookups, which lists each file that calls one |
| The load refuses everything a match can refuse. A match start fails only on session terms: players, tick rate and seed, and the packages they name, which the store must hold and which must load; and on the mode's `on_match_start`, whose runtime failure no load can foresee. | `StartError` has no case of a package's data: its cases are the seed's, the terms', the store's, a slot rule's and the start call's |
| A layer calls a higher layer only through a hook the higher layer registers: a capability adds to the script view through its column, to a call's frame through its part, to a call's effects through its effect types, each of which applies itself, and to the script API through its row of the capability table. | The layer test |
| Every order that matters is by stable id, and every rounding uses one helper. A system that spends something shared, takes ids or runs scripts walks its units through `Ordered`, a scratch that sorts the entities of its query by stable id: a query gives them in archetype order, which a component added to one unit, or a restore, changes. | The archetype-shuffle test |
| Each tick's work has a fixed limit, or a cost in proportion to the units that take part: no tick pays for a scan or a rebuild the other ticks do not. | The work record; the navigation bench |
| Restored state is checked like package data: a restore gives an error for every flaw, never a panic, and what it accepts plays on without one. Its times and counts stay within what a match makes: the tick, every time and every count are at most 2⁶², which no match reaches, so no sum of two overflows; every period is at least a tick; and every relation a system takes between two restored values, or between one and the books, holds, as the start of an attack under way, its resolve less its windup, is no sooner than tick 0. | Every state type's check, a required method of the state traits, which the compiler proves; the snapshot fuzz, which flips each byte of a proving match's snapshot, restores it, and plays five ticks on what restores; the structured fuzz, which puts into that snapshot values of each state type its decode accepts, the edges of every number among them, and does the same; a restore test at the limits of each state type's times and counts |
| Each rule of a network session has one owner on each side, and a client that follows the rules is never refused. | The net scenarios under load |
| `common` depends on no crate but `serde` and `derive_more`. | The manifest test of `common` |
| Only `store` writes a file or starts a thread ([Storage](#storage)): the deterministic core writes no file and starts no thread. | `disallowed-methods` in `clippy.toml`, which fails the check chain at compile time; the storage rules test of `common`, which checks the list and where code expects the lint |

**Books.** A match's books are built by one pure function of its packages and a tick rate, with no world: the unit types and their tags, the tracks, the modifiers and the actions with their params and effect lists, the AIs, and the projectile and area specs. The package load calls it at the fastest rate the manifest allows, where a time counts the most ticks, so what the books cannot hold fails the load; a match calls it at its own rate and puts what it gives in place. The order of every id is the order the builder loads in: the tags, the tracks, every package's modifiers, then each package's actions and unit types, the mode's first. A script is named by its place in the order a match compiles them, and the hooks it defines come from what the load read of it, so no book needs a script host.

## Capabilities

The core has no genre code; a mode combines capabilities, one native mechanism each: [Capabilities](04-capabilities/00-overview.md).

## Lifecycle and sessions

- Engine states: **waiting** (players connect, packages load, start gates pass) → **running** → **ended** (result or aborted; the log is sealed, end hooks run).
- Start gates and end hooks are how optional modules join the lifecycle; the engine has no money code. The `payments` module adds a start gate that locks stakes and an end hook that settles them.
- Everything inside running (pick, rounds, buy time, overtime) is defined by the mode script.
- `ctx.end` is optional: a persistent world never calls it.
- How a session outlives its process, its links and its players: [Sessions](10-sessions.md).
- A session outlives the server process. A match restores by replaying its own log from its latest checkpoint, from tick 0 when it has none; a world loads its latest checkpoint and replays the log after it. Players reconnect with the same main key; a new session key is a renewal the log records.
- A session pauses by running no ticks, and runs at a game speed by running more or fewer ticks a real second; the sim reads no clock, so neither enters the log. Who may pause is the host's setting, and always the player in singleplayer.
- The server writes each input to the log before its tick runs and flushes the log from a background thread, so no tick waits on the disk; a crash loses at most the unflushed inputs, and clients resync to the restored state.
- A match that is not back within the host's restore window aborts. The window must end before any stake's hold invoice expires; a window of 0 means a crash always aborts.
- Script state is versioned; migration hooks convert saved state when a package version changes at restart.

## Inputs

Anything from outside enters the sim as a recorded input: player inputs (commands, each in its capability's format), bot inputs, player connects and disconnects, carry loads, admin commands, payment events, calendar time. If it is not in the log, the sim cannot depend on it.

Clients can join a running game at any time; they receive the current state of what they can see.

## Session log

- A match with no save is one segment; a persistent world checkpoints every few minutes, and every save is a checkpoint. Format: Protocol Spec.
- The verifier replays any segment from its checkpoint. Hosts set how long logs are kept.
- **Snapshot:** the postcard encoding of every sim component and resource, entities sorted by stable id, component types in an order the engine release fixes, script state maps sorted by key. Its format belongs to the engine release, not the protocol, and carries a data version, which a release raises whenever it changes the format.
- **State hash:** one BLAKE3 hash for each state type over the same bytes the snapshot holds, then one hash over the list of `(name, type hash)` ([Determinism Core](09-determinism-core.md)). The goldens compare it on every tick, on every OS in CI, and at the first mismatch the per-type hashes, to name the first divergence.
- **No slow tick for a checkpoint:** at the tick boundary the server copies only the components changed since the last checkpoint; a background thread applies them to its copy, encodes and hashes it, and writes the checkpoint record when done.

## Saves

A save is a checkpoint a player keeps: the snapshot at a tick boundary, and the session log before it. The player asks for one with a command, the mode with `ctx.save()` or at its `[saves] autosave_ms`; a mode that sets `[saves] by = "mode"` refuses the player's, as a hardcore mode does. A save costs no slow tick: it is written as every checkpoint is, from a copy, on a background thread.

- **Load.** Loading a save restores its snapshot and starts a new segment of the same session from it; the log after the save is dropped, so a load goes back in time. A singleplayer session may load any of its saves.
- **Converters.** A save loads on the release that made it and on every later one. Each release carries a converter from the data version before it, which rewrites a snapshot one way, as Factorio's map versions and Minecraft's DataFixerUpper do; loading an older save runs the converters in order. A release may drop the converters older than a point it names, and the launcher then fetches the last release that had them to convert in two steps. Script state converts by the packages' `state_version` and migration hooks, as for a world.
- **Verification.** A segment verifies on the release that recorded it, from its checkpoint. A converted save starts a new segment whose checkpoint the converter made outside the sim: the log names the release before and after, and the proof of the earlier segment ends at the conversion.
- **Carry.** The mode declares a carry schema, typed and versioned like script state. `ctx.carry` reads the carry the session loaded and writes the carry it will hand on; the server writes it out at the session's end and with each save. A session loads a carry as an external input at its start, so its log holds everything it read. A campaign's next mission, and a Diablo character's next play session, start from it.

## Storage

`store` owns how files are written and threads run; the crate that defines a file's bytes owns what they hold. `store` depends on no engine crate.

- **Durable files.** `DurableFile` writes a temporary file, syncs it, renames it over the target and syncs the directory; each step's failure is its own error, never retried ([D9](10-sessions.md#decisions)). `SecretFile` is a durable file of mode 0600 that a read refuses when others may read it, as OpenSSH does: the server's key and its TLS identity are secret files.
- **`protocol` does no IO.** The session log writes framed records into a `RecordSink` and takes the durable count from its caller, so it runs with no disk and no thread, a browser's verifier included. `net`'s `SessionJournal` implements the sink over `store`'s `AppendWriter`.
- **Workers.** Every thread starts through `store`'s `Worker`: named, closed when its handle drops, and joined; a handle dropped during a panic does not join, so a crash ends the process at once. One thread per job, so a snapshot's write never delays the journal's sync. The queues are `AppendWriter` (a stream with grouped syncs and a durable count), `Exchange` (one job in flight) and `LatestWriter` (the newest value only); none waits for the disk or allocates in a steady state.
- **Data directories.** `DataDir` exists only while it holds its directory's lock, so a second server or client on one directory is refused, and it gives every path in it from typed ids.
- **Faults.** A worker's failure is an enum of the step that failed, given once; `store` decides no policy. `net`'s `Faults` applies it: a journal or snapshot failure ends the server with exit code 74, and a failed receipt is logged. A sync slower than a second is logged as `JournalSyncSlow`, and the server plays on.
- **Enforced.** `source/clippy.toml` lists the functions that write files or start threads under `disallowed-methods`, so a call outside `store` fails the check chain. Allowed callers say so with `#[expect(clippy::disallowed_methods, reason = "…")]`: `store` itself, tests, `log`'s JSON file, the LAN check's run directory, and a test's golden file. A test of `common` checks the list and every `expect`.

## Scripting

`script` is its own crate in the deterministic core, so `sim` is testable without Rhai. Scripts run inside the sim tick; the API they call is the registry's ([Script API](08-script-api.md)).

- **Narrow game API.** Scripts never touch the ECS; they call the script API.
- **No `bevy_mod_scripting`.** It exposes all Bevy types and [pins Bevy patch versions](https://lib.rs/crates/bevy_mod_scripting_script).
- **Rhai engine:** `Engine::new_raw` with only the packages scripts need, in strict variables mode ([Engine enums](08-script-api.md#engine-enums)); no `eval`, imports, floats or time; `print` goes to debug logs only; a fixed hashing seed; never `unchecked`.
- **Operation limits:** a limit per call, and per-tick pools, which the manifest sets; counts are the same everywhere, so an over-budget script fails the same everywhere. A property a type reads through its indexer, as `ctx.p.damage` or `unit.params.aggro_range`, costs one operation, as a getter does: the host gives each such type a getter and a setter of each property name its scripts use, which call the indexer, as Rhai would try a getter first, build an error message, and only then call the indexer. A method of a value or a handle, as a position's, a vector's, a unit's or the map's, takes it as a copy: Rhai feeds a value a method may change back through the setter of the property it came from, as `d.target` or `ctx.map`, and these have none, so it would build two error messages and spend two operations more. Each hook runs in one pool:

  | Pool | Hooks |
  | --- | --- |
  | The player's, one per player slot | The calls that player causes: an action of a unit the player controls, its deliveries and modifiers, the player's quests and dialogue |
  | `think` | AI: `on_think`, `on_heard`; the hooks of an action or modifier whose source no player controls, or is gone |
  | `mode` | The mode's hooks: timers, inputs, deaths, region events, `on_level_up`, `on_wake`; a modifier with no source |
  | `systems` | `on_system` |
  | None, only the limit per call | `calc_damage` and `calc_heal`, pure and called once for each damage and each heal, so a crowded fight cannot spend a pool and change how they weigh ([Combat](04-capabilities/combat.md#damage-and-heals)); `on_generate`, under its own larger limit per call, `generate`, since it builds a region in one call |

  A call that finds its pool spent waits and runs first in the next tick, as AI and scripted systems do; a hook that cannot wait, a pure one, has no pool.
- **Other limits** (call depth, sizes) are engine constants, set explicitly, since Rhai's defaults differ between debug and release builds.
- **All or nothing per call.** State writes go to an overlay the call can read back; engine effects (damage, spawn, orders, timers) are queued. On success the overlay commits, then the effects apply in call order. Each capability keeps its own effect types in the call's frame, one queue a type, and each type applies itself: the queue records with each effect the apply of its type, so the script runtime names no capability, and no table can pair a type with another type's apply. On failure (error, overflow, limit) both are discarded, the sim emits a `script_error` event, and the tick goes on.
- **No hidden script state.** It is declared in a typed schema and stored in sim components; see [Script state](03-game-scripting.md#script-state).
- **Measured** (one core of a Ryzen 7 6800U): a hook call through `ScriptHost::call` costs 63.2 ns with an empty body, `script/call`, and 384 ns when the hook makes one call to a function the host registered, `script/native`, so each such call, as each `ctx` query is, costs about 321 ns, 5.1 times the hook's own. The AI's `on_think` calls take 43 % of a 3v3 tick. Each batch of script calls reads the script view first: 72 ns a unit when every unit moved since the last read, `script_view/read`, and 33 ns when one unit in a hundred changed, `script_view/reread`, where a read that fills every row took 1.21 µs a unit.
- **The script view keeps what did not change.** A read fills again only the parts of a row whose source's parts changed since the last read, the core's or a capability's, and copies the rest of the row from the read before, column by column, each run of rows that follow one another at once; a row no source fills again joins one run for every column. Each source finds its changed units through a Bevy query of `Changed` filters, one for each part it reads, which its parts' type derives, so a source cannot read a part whose change the view misses; a part that comes or goes moves the unit to another archetype, which the view compares, and a unit new to the view fills whole. A source's fill sees only the unit's parts, its column and the view's relations: what else a column's rows derive from, as combat's life pool, the column checks as each read starts, and a change of the relations, a book shared at load, or Bevy's check of the world's change ticks fills every row: the check clamps each tick older than Bevy compares exactly, but not the last read of the view's own queries, so an observer of it makes the next read whole. A debug build reads a second time with every row filled, and fails when the two reads differ.

## Backends

Collision, pathfinding and visibility each have one interface and pluggable backends, chosen per mode: [Navigation](04-capabilities/navigation.md), [Vision](04-capabilities/vision.md). Collision, within each layer bodies move on: circles on a plane (now), static 3D level geometry, or `physics`. A physics backend may use strictly deterministic floating point inside itself, such as Rapier's [`enhanced-determinism`](https://rapier.rs/docs/user_guides/rust/determinism/) mode, pinned per release and checked by the goldens on every OS; everything else stays fixed-point, and a NaN panics before it reaches a snapshot.

## Bevy

- `sim` depends on `bevy_ecs` only and is one schedule, which runs on one thread whatever features a build turns on: a tick's systems are too small to share, and Bevy's parallel executor made a 3v3 tick cost 2.5 times as much. The server and client run it inside Lightyear's fixed tick; the verifier and the tests run it in a bare `World`. The server never links the renderer. Pinned to [Bevy 0.19](https://bevy.org/news/bevy-0-19/); the script API and protocol expose no Bevy types.
- Sim systems touch only sim components, so Lightyear's components cannot change a result.
- The server records the inputs received since the last tick, then runs the tick. It hashes the state at checkpoints and at the result; a hash after every tick is opt-in.
- Each unit replicates to the clients whose vision group sees it. A client predicts only what its player controls (position, destination, death and respawn), with no input delay: Lightyear keeps its tick ahead of the server's present tick by half the round trip, a jitter margin, a sync error margin and one tick, so its inputs land in time; it is a full round trip ahead only of the newest server state it holds. A rollback reruns the sim from the server's state. It predicts movement, never a random outcome. The client builds the same books as the server from the packages it holds, at the rate the server's listing names, and installs them with the part of the mode no script runs: the map's metric, bounds, relations and pathing grid, and combat's bindings. So it predicts by the rules the server runs, and keeps no copy of its own. It derives its units' stats, tags and step from their type, level and modifiers, which the server sends, and starts their actions through the core's checks: an attack's or a cast's windup, and the cooldown when it goes off. It runs none of their effects, and no script: damage, launches, costs and a cast's effects come from the server. The server sends the teams' relations as a script changes them, as the client's targets and filters read them. It drops each sent input older than the deepest rollback Lightyear takes.
- `client` draws each unit with its own entity, interpolated between ticks; floats (`Transform`) exist only there.
- **Measured** (one core of a Ryzen 7 6800U): a packet's signature costs 17.5 µs to sign and 25.6 µs to check, `chain_head/sign` and `chain_head/check`; a 3v3 tick at 20 Hz costs 114.4 µs on average and 0.83 ms at worst, `server_tick/mean_3v3` and `server_tick/worst_3v3`, but the first, which builds the schedule and the pathing grid's labels, 1.64 ms, `server_tick/first_3v3`; a re-simulated tick of the lane 1v1 costs 9.4 µs, `client_frame/walk_rollback` less `client_frame/walk` over a rollback's 4 ticks. A 3v3 server tick with 6 packets costs at most about 1.0 ms of its 50 ms, and the first about 1.8 ms. A client's re-simulated 3v3 tick costs 23.0 µs, `client_frame/walk_rollback_3v3` less `client_frame/walk_no_rollback_3v3` over a rollback's 3 ticks, so an 8-tick rollback at a 200 ms round trip costs about 0.18 ms.

**Determinism rules for `sim`** (numbers, RNG and the state hash: [Determinism Core](09-determinism-core.md)):

- One fixed-tick schedule in ordered stages. A pass over what a stage changed runs in the `SimEdge::After` of that stage, and one over what changed between ticks in `SimEdge::Start`; a test fails a system of the proving match's schedule in no stage and no edge set, which would run in a gap in an order Bevy picks. Two systems with conflicting access and no order fail the build. So does a system or set that joins a set it already sits in through another: Bevy only warns of that redundant edge, and a warning fails no test that does not read it. A test builds the proving match's schedule with no automatic sync points, which Bevy adds between systems and which order them in passing, so every order the sim relies on is stated.
- Stable entity ids, never Bevy `Entity`, for the protocol, replays and every order that matters; Bevy [does not guarantee query order](https://docs.rs/bevy_rand/latest/bevy_rand/tutorial/ch02_basic_usage/index.html).
- Positions are 3D and fixed-point, 1 unit = 1 meter, within ±2²⁰ m. No floats outside a physics backend, no randomly seeded hash maps, no wall clock. An overflow panics in every build.
- The RNG is a cryptographic PRF, so clients cannot recover the seed from outcomes. A random value that decides an outcome never reaches a client before the log is published: clients predict effects, never results.

## Creator tools

- **Data schemas.** A JSON Schema for every data file is generated from the same types the engine reads, as the script API reference is generated from the registry, and a test fails when the checked-in schemas differ; an editor such as VS Code then completes and checks a package's TOML as a creator types. Generating them needs a schema crate, to be chosen and approved when the work starts.
- **Hot reload.** A local session in dev mode reloads changed scripts, data and text without a restart, as Roblox Studio and Dota 2's workshop tools do. A reload is no input the log can replay, so a dev session's terms say it is one, the verifier refuses its log, and a dev session takes no payments.

## Testing and diagnostics

- **Match scenarios** run whole matches between scripted players (`OrderScript`) in the test suite, through a modeled link of delay, jitter and loss on a manual clock, so each run repeats. Lightyear measures round trips by the wall clock, which varies with the machine's load, so the harness pins the measured round trip to the model's worst one, twice its delay and jitter, and its jitter to zero: Lightyear's own terms then give the lead that covers every modeled delay. Its frame costs under delay are not real ones.
- **Goldens.** Two pinned records of whole matches prove that a change keeps behaviour: the state golden, a BLAKE3 digest of each tick's state hash, which changes with the state's layout; and the behaviour golden, a digest of each tick's units (id, type, team, position, pools, death), deaths, damage and script failures, which does not. A change of layout alone updates only the state golden; a change of behaviour updates the behaviour golden and names itself. They run on the lane match; on the 3v3, played by scripted players in one match that holds a skirmish of first blood, mend and haste, and a tower that turns on a diver; a camp, whose wolf a hero leashes and two fell; and the lanes, where heroes farm the first waves; and on the proving match: a mode in `packages/test` that uses every capability the release runs, owned by the tests, with no balance to keep. A scripted player's orders name their units by role, as the player's hero or the enemy creep of least life near it, and the match resolves them to stable ids as it sends each. The 3v3's late rules, an inhibitor's fall and its super creeps, the warden's blessing and the core's fall, which no short match reaches at level 1, have a rules test that deals each fatal blow through the runner's world, with no golden and no replay.
- **One scripted match for each mode.** A new rule of a mode's match joins that mode's scripted match: its orders join the script, and its assertion reads its event in its own tick, not the final state. A new run is only for a check that a replayed match cannot hold, such as a change to the world that no input can make, as the late-rules test is: each run pays for the pick and its replay again. The state is hashed once a tick: the golden takes the trail's total.
- **Structure tests**: the layer test, which checks each module's imports against the capability table; the archetype-shuffle test, which plays the proving match with every unit moved to a new archetype before each tick, from the highest stable id down, so that queries meet the units of one archetype in reverse order, and checks both goldens; and the state table test, which checks each capability's state names.
- **Work record**: at the end of each stage of a redesign, the instruction count of the proving match and the 3v3, and the worst tick against the mean; a stage that makes either worse by more than 10 % says why.
- **Benches** (`criterion`, behind each crate's `bench` feature): one target for each crate, whose cases criterion's filter chooses by their `<group>/<case>` id; end, kernel and primitive cases, one for each stage of the tick ([Benches](#benches)). A measurement runs on request; CI runs each case once.
- **LAN check** (`campfire-lan-check`, on request): the real server and two `client --bot` processes on `127.0.0.1`, and a bot with the wrong certificate that must fail and say why, checked from their JSON logs and by the verifier. Every order must take effect in its stamp tick, or in the tick after a frame in which the server caught up with a stall, which the server logs, if the order's stamp is among that frame's ticks. A frame runs at most the max input delay less one ticks, so no order ever becomes late; the time past that is dropped, and the server logs it ([Sessions](10-sessions.md#decisions), D7). Each run keeps its logs in a directory of its own.
- **CI** runs the check chain, each bench case once, and the LAN check on Linux, Windows and macOS; each platform's verifier then replays every platform's session log.
- **Warnings fail tests.** Each test fixture that runs a match holds a `LogCheck` from `log`'s internals: `TestMatch` in `capabilities`, `InProcessMatch` in `net`, and `FixedMatch`, `Arena` and `RestoreTarget` in `runner`; a test that runs a match with none, as the sim's own tests do, holds one itself. A test keeps its fixture while it replays the fixture's log, so the replay runs under the same check. Fixtures hold it, not the sim's world, since a test build also builds the binaries a test runs, and a check in their world would take their log from them. The check is a subscriber for the test's thread, which also takes the records of the `log` crate, so Bevy's warnings reach it, and it keeps each event at Warn or Error. A test that causes one takes it, typed, with `take::<E>()`, and asserts on what it got. When the thread's last check drops, each event no test took fails the test, with its lines, as the LAN check fails a match that logs one. The fixtures of one thread share a check, so a test with two matches has one; a thread that already panics skips it. The check does not see another thread's events: the fixtures run every schedule on the test's thread, and the LAN check reads the rest. It follows rustc's test suite, where a diagnostic with no annotation fails the test, and pytest's `-W error` with `pytest.warns`, where a test names the warning it expects by its type.
- **Logging** goes through `tracing`, never a print; a binary's `--help` and `--version` are its output, not a log, and print. `sim` and `capabilities` log nothing; they report through resources the runner logs. Binaries log to standard error, and to JSON lines with `CAMPFIRE_LOG`. An event a tool or a test reads back is a typed `LogEvent`, with a round-trip test, and so is each warning and error that `net` and `runner` log.

## Benches

- **One target for each crate.** A crate with benches has one `[[bench]]`, `benches/<module>.rs`, whose `criterion_group!` takes `bench::run`, the one function of `lib.rs`'s `bench` facade. Criterion's own command line chooses the cases, as libtest's filter chooses tests: its filter, a regular expression over each case's `<group>/<case>` id; `--exact`; and `--list`. So `cargo bench --workspace --features bench --bench '*'` runs every case, and `-- num/` a group: `--bench '*'` takes only the bench targets, and each crate sets `bench = false` on its library, whose libtest harness refuses criterion's flags. A group's name is unique in the workspace.
- **Fixtures.** Criterion calls a case's function only when the filter takes it, so a fixture that loads packages or starts a match is made in the first case that needs it: a `LazyCell` holds one the cases share and only read, an `Option` filled with `get_or_insert_with` one a case changes. A fixture of plain values reads no file and runs no tick.
- **Three tiers.** An **end** case runs the reference packages as a match plays them, and times one end: the server's tick or frame, or the client's frame, by `iter_custom`, never a step that runs both. The 3v3 for the server's tick and its stages; `InProcessMatch` for the frames of both ends. A **kernel** case runs one capability's work for a tick on a made scene of 1,000 units, the count of an RTS battle ([Genres](04-capabilities/genres.md)), from `KernelScene`: crowded into 40 m square and spread over 120 m square when its cost grows with how close the units stand. A kernel whose units need a mode's kits runs in `runner` on the reference 3v3. A **primitive** case runs one operation over 4,096 inputs drawn from `split_mix`, or over one input when the operation costs more than a microsecond.
- **A case's id.** The group is the path measured, a noun: `server_tick`, `server_stage`, `server_frame`, `client_frame`, `box`, `collision`, `pathing_grid`, `route_planner`, `fog`, `script`, `script_view`, `state_hash`, `snapshot`, `chain_head`, `num`, `root`, `rng`, `vec3`. The case is the workload: an end case's is its statistic and scenario, as `mean_3v3`, `worst_3v3`, `first_3v3`, `walk` and `worst_1v1`; a kernel case's is its scene, `crowded` or `spread`, or its variant when density does not change its cost; a primitive's is its operation. An end has a `mean` and a `worst` case where a match has both, as the worst tick is the budget ([Tick rate](04-capabilities/00-overview.md#tick-rate)); a `mean` case plays a whole match each iteration and states its ticks as its throughput.
- **Every case states its unit.** A case whose iteration runs more than one of its cost unit, units, calls or inputs, states `Throughput::Elements` of that count. Each bench function's doc comment names the path, the scale and the cost unit.
- **A stage case times one stage of the real tick.** `sim`'s `StageClock::install`, which its `internals` give, adds to a built `SimUpdate` schedule a probe at each of the ten edges around the nine stages, which writes the time into a resource outside the sim's state, so the match and its hashes stay the same. Each probe runs after the closing set of the stage before it, so a stage's time holds its `SimEdge::After`; `SimEdge::Start`, `start_tick`, `end_tick` and the session log's part of a tick fall outside every stage. The proving match plays to both goldens with the probes in its schedule.
- **A kernel for each path that passes 10 % of a tick.** Every stage has a case in `server_stage`; a path whose stage passes 10 % of the mean or the worst 3v3 tick also gets a kernel: `think` in `script`, `vision` in `fog`, `resolve` in `script_view`, `inputs` in `pathing_grid` and `move` in `route_planner`. A capability's Cost section names the case that measures it and the figure, with the machine.
- **Profiling.** A profile of a case comes from a build with frame pointers in its own target directory, so the usual build is not rebuilt: `RUSTFLAGS="-C force-frame-pointers=yes" cargo bench -p <crate> --features bench --no-run --target-dir target/frame-pointers`, then `perf record --call-graph fp` on the bench binary with `--bench <id> --profile-time 10`. DWARF unwinding loses the stacks of the optimized build, and AMD processors have no LBR. Frame pointers cost no measurable time here.
- **In CI.** CI runs each case once on every platform, `cargo test --workspace --all-features --bench '*'`, criterion's test mode, so a case that panics, breaks a `debug_assert` or repeats a name in its group fails the run. A case runs as it measures, each `worst` and `mean` case of `server_tick` and `server_stage` a whole match, so the step has its own limit of 60 s in the dev profile, past the 30 s of a test suite.

## Networking

The open protocol is separate from the transport:

- **`protocol`** (versioned, documented): session log (headers, signed inputs, checkpoints, results). Everything a replay needs.
- **`net`**: [Lightyear](https://github.com/cBournhonesque/lightyear) for transport, replication, prediction with rollback, interpolation and interest management. Lightyear replicates through [`bevy_replicon`](https://github.com/simgine/bevy_replicon), whose per-client entity visibility the visibility backend drives. Internal: client and server always run the same engine release.

## Libraries

Exact versions are pinned across the workspace. Each release tag also pins its Rust toolchain (`rust-toolchain.toml`) and `Cargo.lock`, so release builds are reproducible: anyone can rebuild a tag and compare hashes with the published binaries. Hashes cover the unsigned build outputs, because operating-system code signing changes the bytes. Release trust follows [TUF](https://theupdateframework.io/docs/security/): several offline keys, a signature threshold, and revocation.

| Area | Crate | Notes |
| --- | --- | --- |
| Fixed-point numbers, trig, sqrt | Own code in `math` | `Num`: `*` and `/` round to nearest, ties to even; exact decimal parsing; `sqrt` from an `f64` estimate that integer steps correct to the exact root, so the result does not depend on the float; `sin_cos`, `atan2` by series at high internal precision, within 0.501 ulp over the tests' sweeps. `fixed` rounds `*` toward −∞ and constants down, and `fixed_analytics` reaches 48 ulp in `atan2`, so neither is used ([Determinism Core](09-determinism-core.md)) |
| SIMD lanes | Own code in `math` | `I64x4`, `U64x4` and `Mask64x4`, four 64-bit lanes on x86-64-v3 and armv8-a alike, as array loops LLVM vectorizes, exact as the scalar code. Only ops with a vector form on both: no 64 by 64 multiply, a division only through `f64`, exact within 2⁵¹, and conversions between `i64` and `f64` by exact bit tricks, as AVX2 has none. An op wraps, as a checked one would not vectorize; its doc states the domain where it cannot, a debug build asserts it, and a caller checks its inputs once in release and takes its scalar path outside. A caller stays only when it measures faster on x86-64; NEON is not measured, and CI's arm64 runner proves only that the results are the same. `std::simd` is unstable, and intrinsics need `unsafe`. Inline assembly measured three times as slow for each group of lanes, and 10 % faster only as a whole loop, which would need two copies, x86-64 and aarch64 |
| RNG | `blake3` keyed hash, wrapped in `math` | A PRF by specification, counter-based as in [Random123](https://www.thesalmons.org/john/random123/papers/random123sc11.pdf) but cryptographic, unlike Philox; BLAKE3's keyed known-answer vectors run in its tests. Range sampling is own code. No `rand`, which [may change output in minor releases](https://www.rustmax.net/library/rand-book/crate-reprod) |
| Protocol encoding | `postcard` | [Stable wire format](https://postcard.jamesmunns.com) since 1.0 |
| State hashes | `blake3` | At checkpoints and the result; per tick in the goldens |
| File and package fingerprints | `sha2` | SHA-256, as Blossom addresses blobs |
| Error types | `thiserror` | Derives each error's `Display` and `source` from its variants' attributes; a message holds its own step, and `log`'s `ErrorReport` writes the chain |
| Other `Display` impls | `derive_more` | Derives the text of a type that is no error from its `#[display("…")]`, or a newtype's from its inner value |
| Command lines | `clap` | Derives each binary's `Args`, each value through its `FromStr`, with `--help` and exit code 2 for a usage error |
| Nostr | `nostr` in `protocol`, for delegations and their signatures; `nostr-sdk` and `nostr-connect` for the planned `identity` and `ownership` | Still alpha |
| Lightning (`payments`) | `nwc` | Drives the host's own wallet, including [hold invoices](https://getalby.com/blog/build-conditional-payment-logic-into-your-app). No embedded node |
