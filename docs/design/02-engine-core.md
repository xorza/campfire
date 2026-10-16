# Campfire — Engine Core

The engine's structure: its modules, the rules that hold them, and the choices of each engine-wide system. A capability's own mechanism is in [Capabilities](04-capabilities/00-overview.md).

## Modules

| Module | Status | Does |
| --- | --- | --- |
| `common` | built | The plain values crates that do not depend on each other share |
| `derive` | built | The workspace's derives: an enum's array of its members, and their names |
| `codec` | built | The one gateway of serialization, a proxy crate ([Serialization](#serialization)) |
| `schedule` | built | The sim schedules' build settings, a proxy crate |
| `math` | built | Fixed-point numbers and vectors, trig, roots, RNG ([Determinism core](09-determinism-core.md)) |
| `protocol` | built | Signatures, keys, the handshake, input and seed chains, the session log; no IO ([Protocol Spec](05-protocol-spec.md)) |
| `sim` | built | Deterministic state and systems on `bevy_ecs`; no genre code |
| `script` | built | The Rhai host, the script API's registry, and the script view ([Scripting](#scripting)) |
| `capabilities` | built | The top of the capabilities' crates: the release, the books, `Match` |
| `values` | built | Data value types, names and ids |
| `geometry` | built | Exact geometry: metric, bounds, bodies, polygons, grids |
| `state-types` | built | The state types modules declare, and how each is sent |
| `visibility` | built | Who receives each kind of state ([Replication audiences](14-replication-audiences.md)) |
| `declare` | built | How a module declares what it adds to a match |
| `units` | built | The unit model, the map's book and its ground |
| `players` | built | The match's and players' entities, resources and commands |
| `stats` | built | The `stats` capability |
| `actions` | built | The action pipeline and slots |
| `deliveries` | built | Actions' deliveries and their hooks |
| `combat` | built | The `combat` capability |
| `navigation` | built | The `navigation` capability |
| `vision` | built | The `vision` capability |
| `projectiles` | built | The `projectiles` capability |
| `areas` | built | The `areas` capability |
| `abilities` | built | The `abilities` capability |
| `progression` | built | The `progression` capability |
| `production` | built | The `production` capability |
| `interaction` | built | The `interaction` capability |
| `items` | built | The `items` capability |
| `orders` | built | The `orders` capability |
| `mode` | built | The mode: its script, state, choices and spawns |
| `package` | built | A mode's packages, checked against their index; no IO |
| `client-files` | built | The client's files of a package, which the sim never reads |
| `package-disk` | built | A package directory and the package store on disk |
| `runner` | built | Builds and feeds a match; replays a published log |
| `bots` | built | The bots' order scripts |
| `verifier` | built | CLI that replays a session log and checks it |
| `lan-check` | built | The real server and bot clients over QUIC on localhost, on request |
| `zero-hour-check` | built | Imports and checks a Zero Hour install, on request |
| `harness` | built | The session and network tests' match harness; never shipped |
| `rules` | built | The workspace's structural rules as tests; never shipped |
| `test-packages` | built | The workspace's test packages; never shipped |
| `server` | built | The dedicated server's binary |
| `net` | built | Lightyear over `quic`, and the session host ([Networking](#networking)) |
| `quic` | built | QUIC under Lightyear's link ([QUIC transport](15-quic-transport.md)) |
| `log` | built | Log output and the events a tool reads back |
| `store` | built | Files, data directories, threads and the platform layer, a proxy crate ([Storage](#storage)) |
| `launcher` | planned | Fetches, checks and starts the engine release a server names |
| `client` | built | The Bevy client |
| `import` | built | Writes one reproducible package from a game's install ([Zero Hour](12-zero-hour.md)) |
| `content` | planned | Package signatures, pinning, Blossom fetch |
| `editor` | planned | Map and content editors |
| `identity` | planned | Nostr keys, listings, reputation |
| `ownership` | planned | License checks |
| `payments` | planned | Pools, hold invoices, wallet budgets |

- **A crate for each module.** Each module is a crate, `campfire-<module>`, as Bevy's are, so Cargo checks every import against the graph. A test holds the table above equal to the workspace.
- **Layers.** Each crate depends only on lower layers, and no two crates of one layer depend on each other; `LAYERS` lists each capability crate's layer, and a test refuses a layer that is not one above its highest dependency.
- **A pure sim.** `sim` takes state and inputs and gives the next state: no files, packages or signatures. No crate of the deterministic core depends on `store`, and `store` depends on no engine crate.
- **`common` holds only shared plain values.** A type enters `common` only when two crates that do not depend on each other both name it, with construction, parsing, display and serde alone.
- **A name stays beside its member.** An enum derives `NamedEnum`, each member named by its identifier in snake case, as serde's `rename_all` names it, or by its own `#[name = "…"]`, and data and serde read it by those names alone. Not strum, whose names and serde's are two attributes that can drift; not a declarative macro, whose body rustfmt and the IDE read as opaque.
- **Needs and uses.** A capability's dependency on another is a need, without which it does not run, or a use, which it reads when the match has it, as `Module::NEEDS` and `Module::USES` declare; a test holds the crates' dependencies equal to them.
- **The senders' `Order` lives at the top.** `Order` and its `Action`, a sum of each owner's action, are the top crate's, so `orders` depends on no capability whose action it carries.
- **Contracts through `units`.** A capability asks another for work through a part of `units` both read, never by writing the other's state, as 0 A.D.'s components ask `UnitMotion` for a walk: `Dead`, `ForcedMove`, `Destination` with `ArrivedShort`. A change the asker decides and the owner applies is an event of `units` the owner observes, as `LevelRaised`.
- **One published surface.** A crate's `lib.rs` names only its own items, and a caller depends on each type's home crate.
- **Tests live at the top.** A test that builds a whole `Match` lives in the top crate's `tests/`, and one that plays a session or a network match in `harness`, so no crate dev-depends on a crate above it and no test build holds a second copy of a crate, whose types would not be the match's.
- **The core's crates in order.** `script`, `declare`, `units`, `players`, each on its own layer: the script view is the runtime's own data, which the modules fill, so `units` depends on the runtime and never the other way.
- **Game-bound code.** Code that serves one imported game lives in a module named after it, in the crate whose interface it serves, as `import::zero_hour`; nothing depends on it but the place that installs it, and a mechanism a second game needs moves into a general module.
- **One rendering pipeline.** The client draws every game with Bevy's physically based standard material and the generic formats, glTF and the client files; an importer converts its game to them and writes down what it loses, and a need the pipeline cannot say becomes a general feature, never a path for one game.
- **Bots are inputs.** A bot plays an order script of `bots` and produces inputs as a player does, so no replay depends on bot code.
- **Books are one pure function.** One function of a match's packages and a tick rate builds every book, with no world. The load calls it at the fastest rate the manifest allows, where a time counts the most ticks, so what a book cannot hold fails the load; ids follow the builder's order.

## Structural rules

Each rule has a test that fails when it breaks, as a rule only review holds drifts again.

| Rule | Enforced by |
| --- | --- |
| One owner for each fact: a package fact in one immutable book, a match fact in state, a call's fact in its frame. | The declaration audit; the behaviour golden |
| Each module declares once all it adds to a match but its systems, and every consumer reads that one list: the install, the script API's binding, the network's lists and the audit. An install holds only its systems, each in a set of its own module; a fact the install or the load fixes is a fact of the schedule, not a check each tick; a set's order is its phase's, in `Phase::ALL`, as Avian chains its step's sets in one place. | `Declarations` and `ModuleSet` have no default method; the declaration audit; the test that no module names another's set; the strict schedule's ambiguity check; `Release::unlisted_resources` |
| A name becomes an id where it enters, and every lookup by name is a method ending in `named`; the book builder that resolves a name is its check. | The book builder's tests; the name-lookup allowlist; the load's flaw tests |
| A rule of a package's data has one home, the function that tests it, and untrusted data never reaches an `expect`. | The load's flaw tests |
| The load refuses everything a match can refuse: a start fails only on its terms and on `on_match_start`'s runtime failure. | `StartError` has no case of a package's data |
| A layer calls a higher one only through a hook the higher one registers. | Cargo; the layer test |
| No crate re-exports another crate, whole or in part. | `no_crate_re_exports_another_crate` |
| Every order that matters is by stable id: a system that spends something shared, takes ids or runs scripts walks its units through `Ordered`. | The archetype-shuffle test |
| Each tick's work has a fixed limit or a cost in proportion to the units that take part. | The work record; the benches |
| Restored state is checked like package data: every flaw is an error, never a panic; times and counts stay within 2⁶², every period at least a tick. | The `Tick` decode; each state type's check; the snapshot, structured and mutation fuzz |
| Each rule of a network session has one owner on each side, and a client that follows the rules is never refused. | The net scenarios under load |
| A function that acts on a match has one home, by what it acts on: the whole match a trait on `World`, the match's content an associated function of its module's type, a value made from the world a constructor of its own type, a test fixture a struct of the ids it places. | The world rule test of `rules` |
| `common` depends on no crate but `serde`, `derive_more`, its hashers and, behind `internals`, `backtrace`. | The manifest test of `rules` |
| A dev-dependency that turns on `internals` of a normal dependency names a crate whose `internals` gates code of its own. | `a_dev_dependency_turns_on_internals_that_gate_code` |
| A required dependency is one production code names; one only `internals` code uses is optional, and one only tests use is a dev-dependency. | `a_required_dependency_is_named_by_production_code`, `an_optional_dependency_is_named_by_library_code`; cargo's `unused_dependencies` |
| No sibling modules of a crate import each other in a cycle; two types that are one table in two parts share a module. | `no_crates_modules_import_each_other_in_a_cycle` |
| A banned call goes through its proxy crate: files and threads through `store`, formats through `codec`, schedule settings through `schedule`; only a proxy's root exempts the lint, and a proxy that lacks what a caller needs gains it. | `clippy.toml`'s bans; the tests of `rules` on each exemption and proxy |
| Code that differs by OS lives only in `store`'s `platform` module. | The platform rule test |
| A citation of a design decision names a document and a title it holds. | `a_citation_names_a_decision_its_document_holds` |

## Lifecycle and sessions

- **Three engine states.** Waiting, running, ended; what happens inside running, picks and rounds and overtime, is the mode script's, and a persistent world never ends.
- **Gates and hooks for optional modules.** Start gates and end hooks are how an optional module joins the lifecycle, so `payments` locks and settles stakes and the engine holds no money code.
- **A session outlives its process.** A match restores by replaying its log from its latest checkpoint, and the server logs each input before its tick and syncs from a background thread, so no tick waits on the disk ([Sessions](10-sessions.md)).
- **Everything outside is a recorded input.** Player commands, bot inputs, connects, carry loads, admin commands, payment events and calendar time enter the sim as logged inputs; what the log lacks, the sim cannot depend on.

## Session log

- **A snapshot belongs to the release.** A snapshot is the postcard encoding of every state component and resource by stable id, with `StateRegistry::DATA_VERSION`, raised whenever the format changes, as Minecraft's `DataVersion`; a restore refuses another version before it reads a type, and a golden pins each type's layout with the version.
- **A hash for each state type.** One BLAKE3 hash per state type, then one over the list, so a mismatch names its first divergence ([Determinism core](09-determinism-core.md)).
- **No slow tick for a checkpoint.** At a boundary the server copies only what changed since the last checkpoint, and a background thread encodes, hashes and writes it ([Sessions](10-sessions.md)).

## Saves

- **A save is a kept checkpoint.** A save is the snapshot at a tick boundary and the log before it.
- **One-way converters.** Each release converts saves from the data version before it, one way, as Factorio and Minecraft's DataFixerUpper do; an older save runs them in order, and a converted save starts a new segment whose proof starts at the conversion.
- **Carry is an input.** The mode declares a typed, versioned carry schema; a session loads a carry as an external input at its start, so its log holds everything it read.

## Storage

- **`store` owns every file and thread.** One layer gives each read and write one place to keep its promises: a bound on what it reads, a refusal of a link, a durable write, and an error of cases for each failure. The crate that defines a file's bytes owns what they hold.
- **Durable writes.** A write goes to a temporary file, syncs it, renames it over the target and syncs the directory; each step's failure is its own error and is never retried ([Sessions](10-sessions.md)).
- **Secret files.** `SecretFile` is owner-only, gives no `&Path` back, refuses a file others may open, never replaces a secret on create, and holds its bytes in `Zeroizing` buffers.
- **`protocol` does no IO.** The session log writes framed records into a `RecordSink`, which `net` implements over `store`.
- **One thread for each job.** Every thread starts through `store`'s `Worker`, named and joined; none waits for the disk or allocates in a steady state, and each keeps its failure for its owner, so a failure at shutdown reaches the exit code.
- **A data directory is locked.** `DataDir` exists only while it holds its directory's lock, so a second process on one directory is refused, and refuses a directory others may open, as Tor does.
- **Bounded reads, no existence checks.** A read never passes a bound its format's owner derives, and no code asks whether a path exists: it reads or lists, and takes `Missing`, so every answer comes from one moment.
- **No policy in `store`.** A failure is an enum of the step that failed; the caller decides, as `net` ends the server with code 74 on a journal failure.

## Serialization

- **One gateway.** Every serde encoding and decoding goes through `codec`, with one configuration, one canonical form and one error type for each format.
- **Three formats.** Postcard 1.x for every binary encoding, TOML for what a person writes, JSON for logged events and glTF; no other serde format.
- **One encoding of each value.** A decode encodes what it read again and compares, as postcard takes over-long integers and ignores trailing bytes; a whole value refuses any byte past it.
- **No allocation on a hot path.** An encode appends to a caller's `Vec` or writes into a `Sink`.
- **Hand-written layouts for fixed formats.** A layout a specification fixes byte by byte stays with its owner by hand: the importer's file formats, the protocol's signed messages and frames.

## Platform

- **One promise on every OS.** OS-specific code lives only in `store`'s `platform` module behind one interface; a promise one OS cannot keep is narrowed for every OS.
- **Owner-only files.** Mode 0600 or 0700 on Unix; on Windows a protected DACL for the user, a file checked as OpenSSH for Windows checks a key.
- **Durable names.** A rename then a directory sync on Unix, `MoveFileExW` with write-through on Windows, never retried.
- **Stop signals.** The first `SIGINT`, `SIGTERM` or `SIGHUP`, or console event on Windows, raises a flag the app reads; a second ends the process with code 130.
- **Unsafe only where an outside API asks.** Win32 through Microsoft's `windows` crate, `libc` on Unix, each block saying why it is sound.

## Scripting

- **Rhai, not `bevy_mod_scripting`.** `script` alone names Rhai, and scripts call only the registry's API, never the ECS; `bevy_mod_scripting` exposes all of Bevy and pins its patch versions.
- **A raw engine.** `Engine::new_raw` with only the packages scripts need, strict variables, a fixed hashing seed; no `eval`, imports, floats or time.
- **Operation pools.** A limit per call, and per-tick pools the manifest sets, counted the same everywhere, so an over-budget script fails the same everywhere. A player's calls spend the player's pool, AI and sourceless calls `think`, the mode's hooks `mode`; `calc_damage` and `calc_heal` have no pool, so a crowded fight cannot change how they weigh. A mode hook or an AI that finds its pool spent runs first in a later tick; an action's hook fails as over budget.
- **All or nothing per call.** A call's state writes go to an overlay and its effects queue in its frame; on success both apply in call order, on failure both are dropped and a `script_error` is recorded.
- **Declared script state.** Script state is declared and stored in sim components, one `StateSchema` per owner, so no script keeps hidden state ([Script state](03-game-scripting.md#script-state)).
- **The view refills what changed.** A read of the script view fills again only the rows whose sources changed, found by `Changed` filters derived from the parts' types; a debug build reads twice and fails when the reads differ.

## Backends

- **One interface, pluggable backends.** Collision, pathfinding, movement and visibility each have one interface, and an imported game's own behaviour is a backend of it ([Navigation](04-capabilities/navigation.md)). A physics backend may use strictly deterministic floating point inside itself, as Rapier's `enhanced-determinism`; everything else is fixed-point.

## Bevy

- **The sim is one schedule on one thread.** `sim` uses `bevy_ecs` alone, on one thread: a tick's systems are too small to share, and the parallel executor made a 3v3 tick cost 2.5 times as much.
- **Every schedule of the server runs on one thread.** `SimServerPlugins` sets the single-threaded executor on each schedule as the app finishes, as Cargo turns `multi_threaded` on wherever a build's member enables it, and Lightyear's unordered systems would otherwise run in an order set by timing.
- **A match is a world the release built.** `Release::build(world, spec)` is the one build; the `App` owns the world on the server and the client, as Lightyear replicates only its main world, and `Match` everywhere else. The match's operations are traits on `World`, so every host reads the same.
- **A world with no `App` clears its trackers.** A replay or a test's `Match` clears change trackers after each tick, as an `App`'s update does, so a long replay holds removals in bounded memory.
- **A client predicts what its player controls.** Lightyear keeps the client's tick ahead of the server's; a rollback reruns the sim from the server's state; it predicts movement, never a random outcome, and runs no effect and no script.
- **Floats only in the client.** `Transform` exists only in the client, which draws each unit interpolated between ticks.
- **Determinism rules.** Ordered stages, with a strict schedule that fails on an ambiguity or a redundant edge; stable entity ids, never Bevy's `Entity`, as Bevy does not guarantee query order; fixed-point positions within ±2²⁰ m; no floats, seeded hash maps or wall clock; a cryptographic PRF as the RNG, so no client can recover the seed ([Determinism core](09-determinism-core.md)).

## Client

- **Package assets through a package source.** Models and textures reach Bevy through `package://<place>/<path>`, whose reader checks each file against its package's index, and Bevy's own glTF and KTX2 loaders parse them on demand.
- **At most four glTF loads at a time.** Bevy's glTF loader waits on a task scope, and its thread runs the next queued load inside that wait (Bevy #21954), so the package source takes one of four permits before it opens a `.glb`, and a load that waits yields instead of nesting.
- **A material file where glTF cannot say it.** `client/materials/<name>.toml` overrides a glTF material whole with what glTF lacks, an additive or multiplying blend, a blend with an alpha test, a depth bias for a layer, drawn by an extension of Bevy's standard material; it is no lighting rule, as every material lights alike.
- **A terrain is quads over the height grid.** `client/maps/<name>/terrain.bin` gives each cell a ground quad with its own texture coordinates and diagonal, and overlays over them in drawing order, so it holds an old tile terrain, Zero Hour's tiles, blends and stretched cliffs, as its renderer draws them, and a splat terrain as overlays; it names no rule of a game. Coordinates are `f32`, as precision comes before the disk: sixteenths of a texel would halve the size and round a cliff's stretched coordinates.
- **The terrain's texture is composed at load** from copies of package textures and border wraps, so the package holds no second copy of its texels.
- **The terrain is lit as every model is,** by the map's lights and normals from the height grid; no game's lighting rule enters the client.
- **No glTF or splat for the ground.** A glTF of each map's ground costs four times the terrain file; a splat terrain of tiled layers, as Unity's and Unreal's are, cannot hold a tile terrain, as each cell draws its own tile.
- **The drawn ground is the terrain's.** A unit, a client object and the pointer's ray stand on the triangles the terrain draws, which may differ from the sim's ground, as the original's logic and renderer differ ([Zero Hour projectiles](20-zero-hour-projectiles.md)).
- **A selection of the player's own units.** A left click selects one, a drag selects those in the rectangle once the cursor moves past 4 pixels, Windows' default; a right click orders the selection as one group, as StarCraft II's does.
- **A command panel from the actions.** The panel shows the first selected unit's train and build actions and its queue, from the sim's own data, so any game's production plays with no panel of its own.
- **Text by Bevy's own UI.** `bevy_ui` and `bevy_text` with the embedded font add no crate outside Bevy, where `bevy_egui` would be a new dependency and a second UI system.
- **Every key on a keyboard with no keypad.** The camera turns and zooms by Alt with the arrows beside Zero Hour's keypad keys, and Space pauses a local match as in Paradox's games; the middle button pans, keeping the grabbed ground under the cursor, as Unity, Unreal and StarCraft II do, and orbits with Ctrl, as Blender's turntable.

## Creator tools

- **Data schemas.** A JSON Schema for every data file, generated from the engine's types, planned, so an editor completes and checks a package's TOML.
- **Hot reload is unverifiable.** A dev session reloads scripts and data without a restart, as Roblox Studio does; its terms say so, and the verifier refuses its log.

## Testing and diagnostics

- **Scenarios on a modeled link.** Whole matches between scripted players run through a link of delay, jitter and loss on a manual clock, so each run repeats; a match played with its links reversed proves the server orders joins and inputs by key and slot.
- **Two goldens.** The state golden pins each tick's state hash and moves with the layout; the behaviour golden pins a digest of each tick's units, deaths and resources, and does not.
- **One scripted match for each mode.** A new rule of a mode joins its scripted match, the one test exempt from the 1 s bound; the suite keeps 30 s.
- **nextest.** A process per test, as libtest cannot end a stuck test; a test is slow past 1 s, and no profile stops at the first failure.
- **Allocation audits.** A crate's `tests/alloc` counts each hot path's allocations a frame at a time, zero unless a fixture names each one it allows.
- **One test feature.** What only tests and benches build is behind `internals` and one gate; `#[cfg(test)]` holds a crate's own test modules alone, and every test build turns `internals` on.
- **The archetype shuffle.** A test moves every unit to a new archetype before each tick and checks both goldens, so no order depends on archetypes.
- **The work record.** A redesign records the instruction count of the proving match and the 3v3; a stage that makes either worse by more than 10 % says why.
- **The real binaries on localhost.** The LAN check runs the shipped server and clients over QUIC and checks their logs with the verifier.
- **Warnings fail tests.** A fixture that runs a match holds a `LogCheck`, and each warning no test takes fails it, as rustc's tests and pytest's `-W error` do.
- **Typed log events.** An event a tool reads back is a `LogEvent` with a round-trip test; `sim` and `capabilities` log nothing, and report through resources.

## Benches

- **One bench target for each crate,** whose cases share one id scheme, `<kind>/<group>/<case>`, chosen by criterion's filter.
- **One selection for every run.** The bench lane builds exactly the crates with a `[[bench]]`, each with `internals`, so a case builds with the same features whichever cases run.
- **Atomic and integration cases.** An atomic case runs one operation over 4,096 inputs; an integration case runs a system for a tick on a scene of 1,000 units, the scale of a large Zero Hour battle, or one end on the test packages.
- **The worst tick is the budget.** An end has a `mean` and a `worst` case, as the worst tick sets the budget ([Tick rate](04-capabilities/00-overview.md#tick-rate)).
- **Stage probes keep the hashes.** `StageClock` times each stage of the real tick from outside the state.
- **Frame pointers for profiling.** Profiling builds with frame pointers in its own target directory, as DWARF unwinding loses the optimized stacks and AMD has no LBR.
- **Code placement moves a case.** The same machine code at another cache-line offset runs at another speed, so a change of a few percent is read against the machine code, not the time alone ([Math performance](../research/math-performance.md)).

## The driver

- **One owner of every cargo invocation.** `source/drive.py` runs every check, test, doc build and bench, as each shape of a cargo command keeps a cache of its own and a `-p` shape rebuilds tens of units for no speed.
- **A lane is one cache shape.** Each lane covers the workspace and runs `--locked`; no command takes a crate list.
- **The ship lint leaves out what unifies `internals`.** A member whose normal dependency turns `internals` on is left out of the shipped build's lint, which `lint` covers.
- **A check runs again only what a change reaches.** A step that passed keeps the tree of files it passed on, and runs again only when a file it reads changed; a test step runs only the crates a change reaches and their dependents.
- **Measured, not assumed.** Each step appends its time and result to `target/drive/history.jsonl`.

## Networking

- **The protocol is apart from the transport.** `protocol` is the versioned, documented format a replay needs; `net` uses Lightyear over `bevy_replicon` for transport, replication, prediction and interest, and links over QUIC alone ([QUIC transport](15-quic-transport.md)).
- **One release on both ends.** Client and server always run the same engine release.

## Libraries

Exact versions are pinned; each release tag pins its toolchain and `Cargo.lock`, so a build can be compared with the published binaries. Release trust follows [TUF](https://theupdateframework.io/docs/security/).

| Area | Crate | Why |
| --- | --- | --- |
| Fixed-point numbers, trig, roots, SIMD lanes | Own code in `math` | Exact and portable ([Determinism core](09-determinism-core.md)); `std::simd` is unstable and intrinsics need `unsafe` |
| RNG | `blake3` keyed hash | A PRF by specification, counter-based as Random123; `rand` may change output in minor releases |
| Binary encoding | `postcard` | A stable wire format since 1.0 |
| Data files, log lines | `toml`, `serde_json` | Through `codec` |
| Client files' vectors | `bevy_math` | The client draws them with no conversion; the sim's vectors are `math`'s |
| State hashes | `blake3` | |
| File and package fingerprints | `sha2` | SHA-256, as Blossom addresses blobs |
| Secret buffers | `zeroize` | Already in the build through `rustls` |
| QUIC | `quinn-proto`, `quinn-udp`, `polling` | QUIC with no IO and no async runtime ([QUIC transport](15-quic-transport.md)) |
| Stop signals | `signal-hook` | A handler that does only what POSIX allows |
| Port mapping | `igd-next`, `netdev`, `socket2` | UPnP-IGD by its blocking API, and the default gateway for PCP and NAT-PMP |
| TLS | `rustls` over `ring` | TLS 1.3 alone, a raw pinned Ed25519 key |
| Errors, `Display` | `thiserror`, `derive_more` | |
| Command lines | `clap` | |
| Nostr | `nostr`, `nostr-sdk`, `nostr-connect` | Still alpha |
| Lightning | `nwc` | Drives the host's own wallet, hold invoices included; no embedded node |
