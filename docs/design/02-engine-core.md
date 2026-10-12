# Campfire — Engine Core

## Module map

```mermaid
flowchart TB
    subgraph apps["Apps"]
        client
        server
        verifier
        import
        launcher:::planned
        editor:::planned
    end
    subgraph checks["Checks"]
        lancheck["lan-check"]
        zerohourcheck["zero-hour-check"]
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
        schedule
    end
    client --> net
    server --> net
    net --> runner
    verifier --> runner
    import --> package
    import --> store
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
    sim --> schedule
    math --> common
    protocol --> common
    classDef planned stroke-dasharray: 5 5
```

Dashed: planned. `log` is left out: the binaries, `net` and `verifier` use it. `codec` is left out too: every crate that serializes uses it. Each layer uses the layers below it.

## Modules

| Module | Status | Does |
| --- | --- | --- |
| `common` | built | The vocabulary of crates that do not depend on each other: the player slot, ticks, segment seed, 32-byte hex values, a package's fingerprint, the TLS key a server presents and a TLS session's channel binding, exit statuses; behind `internals`, the allocation audit of each crate's `tests/alloc` |
| `codec` | built | The one gateway of serialization, a proxy crate: postcard, TOML and JSON, each with one configuration, one canonical form and one error type ([Serialization](#serialization)) |
| `schedule` | built | The build settings of the sim's schedules, a proxy crate: a schedule that fails on ambiguity and redundant edges, and the probes' variant with no sync points |
| `math` | built | Fixed-point numbers, 3D vectors, exact ground-plane vectors, trig, exact integer roots, exact 256-bit products and sums, counter-based RNG, SIMD lanes |
| `protocol` | built | Signatures, session key delegations, the handshake, input chains, the seed chain and the session log, with no IO ([Protocol Spec](05-protocol-spec.md)) |
| `sim` | built | Deterministic state and systems on `bevy_ecs`; no genre code |
| `script` | built | The Rhai host: compiles scripts and runs each call under its limits; the script API itself is in `capabilities` ([Script API](08-script-api.md)) |
| `capabilities` | built | Mechanisms a mode combines, a module each ([Capabilities](04-capabilities/00-overview.md)) |
| `package` | built | Reads a mode's packages and their dependencies, each file on demand and checked against its index, and runs the load checks; writes a package and its index |
| `runner` | built | Builds a match from checked packages and feeds it inputs; owns `SessionRules`, which builds a session's terms and checks them on the server, the client and the verifier |
| `verifier` | built | CLI: replays a session log segment and checks the result |
| `lan-check` | built | The real server and bot clients over QUIC on `127.0.0.1`, on request ([Testing and diagnostics](#testing-and-diagnostics)) |
| `zero-hour-check` | built | Imports the Zero Hour install `GENERALSZH_DATA` names and checks it, on request |
| `server` | built | The dedicated server's headless binary: its host config from the command line, logging, socket, port map and exit; `net` hosts its session |
| `net` | built | Lightyear over `quic`: handshake, replication; and the session host the dedicated server, the client's local server on a thread and the test harness share: lifecycle, lobby, saves, journal and checkpoints; internal ([Networking](#networking)) |
| `quic` | built | QUIC alone under Lightyear's link: an endpoint on a thread of its own, the server's raw TLS key and the client's pin, and the server's port mapping; no game ([QUIC transport](15-quic-transport.md)) |
| `log` | built | Log output: text on standard error and JSON lines into a file; the events a tool reads back; the end of a command line |
| `store` | built | Durable and secret files, data directories and their locks, and worker threads, a proxy crate ([Storage](#storage)); the platform layer, which also catches the stop's signals ([Platform](#platform)) |
| `launcher` | planned | Fetches, checks and starts the engine release a server or replay names; server browser |
| `client` | built | Bevy app: rendering, input, UI, audio, prediction, presentation systems ([Zero Hour](12-zero-hour.md#the-client)) |
| `import` | built | Reads a game's install through `store` and writes one reproducible package, a module for each game ([Zero Hour](12-zero-hour.md#the-importer)) |
| `content` | planned | Package signatures, pinning, cache, Blossom fetch |
| `editor` | planned | Map and content editors |
| `identity` | planned | Nostr keys, session keys, listings, reputation |
| `ownership` | planned | License checks (optional) |
| `payments` | planned | Pools, hold invoices, wallet budgets (optional, separate repo) |

A module is built when the workspace has its crate, `campfire-<module>` in `source/crates/` or `source/checks/`; a test fails when this column and the workspace differ. `lan-check` and `zero-hour-check` live in `source/checks/`, and nothing depends on them.

`sim` is pure: state and inputs in, next state out; no files, packages or signatures.

**Dependencies.** `server` and `client` → `net` → `runner` → `package` → `capabilities` → `script`, `sim` → `math` → `common`; `sim` → `schedule`; `runner` → `protocol` → `common`; `net` → `quic` → `common`, `store`, `log`; every crate that serializes → `codec`; `verifier` → `runner`; `import` → `package`, `store`. `server`, `client`, `net`, `package`, `verifier` and `lan-check` use `store`, and the runner's `internals` for golden files; `zero-hour-check` uses `import`, `package`, `store` and the runner's `internals`; `client`'s tests use `import`'s `internals` for the KTX2 files the client must read. No crate of the deterministic core depends on `store`, and `store` depends on no engine crate. `log` depends on `common`, and on `store` only under its `start` feature, which only the binaries turn on. A type enters `common` only when two crates that do not depend on each other both name it, as a plain value: construction, parsing, display and serde, and no other logic. `common` depends on `serde`, `derive_more` and `thiserror` alone, and, for the allocation audit behind `internals`, `backtrace`.

**Game-bound code.** Code that serves one imported game lives in a module named after it, in the crate whose interface it serves: Zero Hour's importer in `import::zero_hour`, its backends in `navigation::zero_hour`, its AI in the `zero_hour` module of the crate that drives server bots. Nothing depends on it but the place that installs it; a mechanism a second game needs moves into a general module.

Outside the engine crates: the MOBA test content, the rules packages and bots. Only the tests of `package` and `runner`, and `lan-check`, use the MOBA and the bots. Bots produce inputs like players, so replays never depend on bot code.

## Structural rules

Each rule has a test that fails when it breaks, as a rule only a review checks drifts again.

| Rule | Enforced by |
| --- | --- |
| One owner for each fact: a package fact in one immutable book, a match fact in state, a call's fact in its frame. Nothing else holds a copy. | The declaration audit; the behaviour golden |
| Each module, the core's and each capability's, declares once all it adds to a match but its systems: its state types, kinds, parts and pairs, restore checks, books, script view columns, call parts, players' parts and script API. Every consumer reads that one list: the install, the script API's binding, the network layer's lists and the declaration audit. An install asks the declared capabilities what the match has, never another module's resources. | `Declarations` has no default method; the declaration audit test; `Books::install`, which refuses a book its installed owner did not declare |
| A name becomes an id where it enters. After the load no system looks up a name; a script call resolves its name once, with no allocation. Every lookup of an id by name is a method ending in `named`. | The book builder's tests; the allowlist test of name lookups |
| The load refuses everything a match can refuse. A match start fails only on its terms (players, tick rate, seed, the packages they name) and on `on_match_start`'s runtime failure. | `StartError` has no case of a package's data |
| A layer calls a higher one only through a hook the higher one registers: a column of the script view, a part of a call's frame, an effect type, a row of the capability table. | The layer test |
| Every order that matters is by stable id, and every rounding uses one helper. A system that spends something shared, takes ids or runs scripts walks its units through `Ordered`, sorted by stable id, as a query gives archetype order, which a component added or a restore changes. | The archetype-shuffle test |
| Each tick's work has a fixed limit or a cost in proportion to the units that take part: no tick pays for a scan or a rebuild the others do not. | The work record; the navigation bench |
| Restored state is checked like package data: every flaw is an error, never a panic, and what it accepts plays on. Times and counts stay within what a match makes: at most 2⁶², so no sum of two overflows; every period at least a tick; and every relation a system takes between restored values, or with the books, holds. | `Tick` and `Ticks` decodes refuse past 2⁶²; every state type's required check; the snapshot fuzz, which flips each byte of a proving match's snapshot and plays five ticks on what restores; the structured fuzz of every decodable value; the mutation fuzz, which removes, moves and copies each state type's value; restore tests at each type's limits |
| Each rule of a network session has one owner on each side, and a client that follows the rules is never refused. | The net scenarios under load |
| A function that acts on a match has one home, by what it acts on. The whole match, its ticks, its state, its schedule and its session, and the fast access to one module's singleton or index, are methods of a trait on `World`, so a test's `Match` and an `App`'s world read the same: each module owns its traits in `<module>_world.rs`, `<Module>World` for other crates, `<Module>WorldCrate` for its own crate and `<Module>WorldInternals` for the tests and benches of any crate. A world method knows no fixture, and acts on every match. The match's content, what a module loads, spawns, gives, queues or finds by name, is an associated function of the module's type, its capability type where it has one, the world its first parameter: `Actions::load(world, …)`, `Combat::queue_damage(world, …)`, `Units::spawn(world, core, parts)`. A value or a view made from the world is a constructor of its own type, `StateCopy::of(world)`, `PoolMeters::of(world, entity)`; a match in a world of its own is an associated function of the type of the module that builds it, which returns the `Match`, `Session::start_match`, `MatchBuild::arena`. No `pub` or `pub(crate)` free function takes the world, but a system, an observer, a hook or a run condition, which Bevy registers by value, and a function a table holds. A test fixture is a struct of the ids it places into a match, `Carrier::place(&mut game)`, whose verbs take the match, so fixtures share one match and work in any match of their capabilities. | The world rule test of `common`, which scans every source file |
| `common` depends on no crate but those its row above names. | The manifest test of `common` |
| A banned call goes through its proxy crate: files and threads through `store` ([Storage](#storage)), the formats through `codec` ([Serialization](#serialization)), a schedule's build settings through `schedule`. A proxy exempts the lints at its crate root alone, depends on its listed crates alone and names no other group's call; a ban with no proxy is exempt nowhere. When a proxy lacks what a caller needs, the proxy gains it. | `disallowed-methods` and `disallowed-types` in `clippy.toml`, grouped by owner, and the tests of `common` that check each group, each exemption, each proxy's dependencies and each proxy's names |
| Code that differs by OS lives only in `store`'s `platform` module ([Platform](#platform)). | The platform rule test of `common`, which scans every source file |

**Books.** One pure function of a match's packages and a tick rate, with no world, builds its books: unit types and tags, tracks, modifiers, actions with their params and effect lists, AIs, projectile and area specs. The load calls it at the fastest rate the manifest allows, where a time counts the most ticks, so what the books cannot hold fails the load; a match calls it at its own rate. Ids follow the builder's order: tags, tracks, every package's modifiers, then each package's actions and unit types, the mode's first. A script is named by its place in the order a match compiles them, and its hooks come from what the load read, so no book needs a script host.

## Capabilities

The core has no genre code; a mode combines capabilities: [Capabilities](04-capabilities/00-overview.md).

## Lifecycle and sessions

- Engine states: **waiting** (players connect, packages load, start gates pass) → **running** → **ended** (result or aborted; the log is sealed, end hooks run). Everything inside running (pick, rounds, overtime) is the mode script's; `ctx.end` is optional, and a persistent world never calls it.
- Start gates and end hooks are how optional modules join the lifecycle: `payments` adds a gate that locks stakes and a hook that settles them; the engine holds no money code.
- A session outlives its process, its links and its players ([Sessions](10-sessions.md)): a match restores by replaying its log from its latest checkpoint; players reconnect with the same main key. The server logs each input before its tick runs and syncs the log from a background thread, so no tick waits on the disk.
- The restore window must end before any stake's hold invoice expires; a window of 0 means a crash always aborts.
- Script state is versioned; migration hooks convert saved state when a package version changes at restart.

## Inputs

Anything from outside enters the sim as a recorded input: player commands, bot inputs, connects and disconnects, carry loads, admin commands, payment events, calendar time. What is not in the log, the sim cannot depend on. A client may join a running game at any time and receives the current state of what it may see.

## Session log

- A match with no save is one segment; a world checkpoints every few minutes, and every save is a checkpoint. The verifier replays any segment from its checkpoint; hosts set how long logs are kept. Format: [Protocol Spec](05-protocol-spec.md#session-log).
- **Snapshot:** the postcard encoding of every state component and resource, entities by stable id, types in an order the release fixes. Its format belongs to the engine release, not the protocol, and carries `StateRegistry::DATA_VERSION`, a little-endian `u32` after its tag, raised whenever the format changes, a type's layout included, as Minecraft's `DataVersion` is; a restore refuses another version with `SnapshotError::OtherVersion` before it reads a type. A test pins each type's layout fingerprint with the version, in the runner's `tests/golden/layout.txt`, so a layout that changes at the same version fails by name; `CAMPFIRE_BLESS=1` writes the file again only for another version.
- **State hash:** one BLAKE3 hash for each state type over the snapshot's bytes, then one over the list of `(name, type hash)` ([Determinism Core](09-determinism-core.md)). The goldens compare it every tick on every OS, and at a mismatch the per-type hashes name the first divergence.
- **No slow tick for a checkpoint:** at the boundary the server copies only the components changed since the last checkpoint, and a background thread applies them to its copy, encodes, hashes and writes it ([Checkpoints](10-sessions.md#checkpoints)).

## Saves

A save is a checkpoint a player keeps: the snapshot at a tick boundary and the log before it, written as every checkpoint is. When it is due, and loads, are [Saves and loads](10-sessions.md#saves-and-loads).

- **Converters.** A save loads on its release and every later one. Each release carries a converter from the data version before it, one way, as Factorio's map versions and Minecraft's DataFixerUpper do; an older save runs them in order. A release may drop old converters, and the launcher then fetches the last release that had them to convert in two steps. Script state converts by the packages' `state_version` and migration hooks.
- **Verification.** A segment verifies on the release that recorded it. A converted save starts a new segment, whose checkpoint the converter made outside the sim; the log names both releases, and the proof of the earlier segment ends at the conversion.
- **Carry.** The mode declares a carry schema, typed and versioned like script state. `ctx.carry` reads the carry the session loaded and writes the one it hands on; the server writes it at the session's end and with each save. A session loads a carry as an external input at its start, so its log holds everything it read.

## Storage

`store` owns how files are read, listed, checked and written and how threads run; the crate that defines a file's bytes owns what they hold. One layer gives each read and write one place to keep its promises: a bound on what it reads, a refusal of a link, a durable write, and an error of cases for each failure.

- **Durable files.** `DurableFile` writes a temporary file, syncs it, renames it over the target and syncs the directory; each step's failure is its own error, never retried ([D9](10-sessions.md#decisions)). `create_dir_all` makes each missing directory above a path; `create_new_dir` one only where nothing holds its name.
- **Secret files.** `SecretFile` is a durable, owner-only file: the server's key, from which its TLS key derives, a client's `--key`. It is made from a path where the path enters and gives no `&Path` back, so code that holds a secret cannot pass it to `fs::read`.
  - **Read** opens the file, following a link as OpenSSH does, and checks the open handle; it refuses, each as its own case, a missing file, one that is not regular, one others may open, and one longer than its caller's bound, read no further than one byte past it.
  - **Create** writes a new secret and never replaces one: its bytes take the secret's name only if no file holds it, so of two processes that make one key at once, one makes it and the other reads it.
  - **Replace** writes as `DurableFile` does, for a secret renewed under its data directory's lock.
  - **Zeroed.** The bytes a read gives and a caller encodes are `Zeroizing` buffers, which overwrite their memory when dropped, allocated once at the caller's bound plus one byte, so no growth leaves a copy. A decoded key stays in memory while the process runs.
- **`protocol` does no IO.** The session log writes framed records into a `RecordSink` and takes the durable count from its caller; `net`'s `SessionJournal` implements it over `store`'s `AppendWriter`.
- **Workers.** Every thread starts through `store`'s `Worker`: named, closed when its handle drops, and joined; a handle dropped during a panic does not join, so a crash ends the process at once. One thread per job: `AppendWriter` (a stream with grouped syncs and a durable count), `StreamWriter` (no sync, a bounded buffer that makes its caller wait), `Exchange` (one job in flight) and `LatestWriter` (the newest value only). None waits for the disk or allocates in a steady state.
- **Data directories.** `DataDir` exists only while it holds its directory's lock, so a second process on one directory is refused, and gives every path in it from typed ids; its layouts are `ServerLayout` and `ClientLayout`, which a tool reading a finished run's directory uses with no lock. A directory others may open is refused, as Tor refuses its `DataDirectory`.
- **Reads.** `InputFile` reads a file never past a bound its format's owner derives from that format's limits: `SessionLog::MAX_FILE_LEN`, `StateRegistry::MAX_SNAPSHOT_LEN`, `SessionPrivate::MAX_FILE_LEN`, `Nsec::MAX_FILE_LEN`, `OrderScript::MAX_FILE_LEN`. `read` gives the bytes; `read_if_present` none for a missing file; `read_text` UTF-8; `read_stamped` the bytes and the time of change from one handle; `stream` the bytes in pieces, as a package reads a file no further than its index row; and `ranges` any range from one handle, with its `FileStamp`, its length and time of change, by which a reader that opens it again knows it is the same file, as `make` does. A read checks the path's kind before it opens, as an open of a pipe waits for a writer, and the handle's after, and reads at most one byte past the bound, so a growing file is refused. Its error, `ReadError`, is `Missing`, `NotFile`, `NotDir`, `TooLarge`, `Short`, `NotText`, `Exposed` or `Read`. Every error of `store` names its path, so a caller wraps none.
- **Listings.** `DirEntries::read` gives a directory's entries sorted by name, each with its kind, a link not followed; a missing directory is `Missing`. No call asks whether a path exists: a caller lists once, so every name's answer comes from one moment.
- **Child output.** `OutputFile::stdio` makes a new owner-only file as a child process's `Stdio`, so no `File` leaves `store`.
- **Scratch.** A test's files live in `store`'s internals' `Scratch`: an owner-only temporary directory, removed when it drops, which makes, reads, lists, renames, links, exposes and removes files by paths within it, panicking with the path on a failure. A test reads the workspace's sources through `SourceFiles`, and races two writers through `Race`.
- **Faults.** A worker's failure is an enum of the step that failed; `store` decides no policy. `net`'s `Faults` ends the server with exit code 74 on a journal or snapshot failure, and logs a failed receipt and a sync slower than a second.
- **Enforced.** `source/clippy.toml` bans, under `disallowed-types`, every type that holds a file (`std::fs`'s `File`, `OpenOptions`, `DirBuilder`, `ReadDir`, `DirEntry`, `Metadata`, `tempfile`'s `TempDir` and `NamedTempFile`), and under `disallowed-methods` `std::fs`'s free functions, `Path`'s probes, `tempfile`'s functions and the thread starts. Only `store`'s crate root `expect`s both lints ([Structural rules](#structural-rules)); a test of `common` refuses any other `expect` or `allow` of either but the other proxies' roots. Bevy's asset server reads through a package asset source over `package` and `store` ([Zero Hour](12-zero-hour.md#the-client)).

## Serialization

Every serde encoding and decoding passes through the proxy crate `codec`, as every file passes through `store`: one configuration, one canonical form and one error type for each format.

- **Formats.** Postcard 1.x for every binary encoding: the protocol's records, the log, the journal, snapshots and the state hash, commands, the package index, a package's binary files. TOML for data files and configuration a person writes. JSON for logged events, reading log lines back, and glTF's JSON. No other serde format.
- **One encoding.** Postcard's decoder takes an over-long integer and ignores trailing bytes, so the gateway's decode encodes what it read again into a comparing output that allocates nothing: a decode on a hot path costs a second pass and no allocation. A whole value refuses any byte past it. A format's own order rules, as a snapshot's entities by stable id, stay with the format's owner.
- **Sinks.** An encode appends to a `Vec` its caller keeps, or writes into a `Sink` in batches of 64 bytes, so no encode on a hot path allocates.
- **Errors.** A postcard decode fails with `BinaryError`: bytes that end inside the value, bytes of no value of its type, or another encoding of a value. A TOML or JSON parse fails with its crate's error, with line and column.
- **Network.** Every message registers through `register_message_custom_serde`, and every component through `replicate_with` or `replicate_once_with`, with the gateway's encode and decode, at the `lightyear_serde` version Lightyear builds.
- **Outside the gateway.** Layouts a specification fixes byte by byte stay with their owner, by hand: the files the importer reads or writes (`.big`, map chunks, W3D, DDS, KTX2, GLB) and the protocol's signed messages and frames. One engine layout stays hand-built: the golden harness's digest of a tick.
- **Enforced.** `source/clippy.toml` bans, outside `codec`, each format crate's entry points and replicon's postcard helpers, and everywhere Lightyear's registrations that take its own encoding; each format's `Serializer` and `Deserializer` are banned as types. `str::parse` into a `toml::Table` or `serde_json::Value`, and a struct built by its path with no `use`, stay open, as clippy cannot ban them; no code takes either. `common`'s tests check the lists ([Structural rules](#structural-rules)).

## Platform

One promise on every OS. Code that differs by OS lives only in `store`'s `platform` module, `platform/unix.rs` and `platform/windows.rs`, behind one interface: no other code names an OS (`cfg` of `unix`, `windows`, `target_os`, `target_family`, or `std::os`). A promise one OS cannot keep is narrowed for every OS.

- **`OwnerOnly`** makes a file or directory only the current user may open, and checks that a file is so. On Unix, mode 0600 or 0700, given as it is made; a file is owner-only when its owner is the effective user or root and its mode grants others nothing. On Windows, a protected DACL with one entry for the token's user; a file is owner-only when its owner and each allowing entry name that user, SYSTEM, Administrators or TrustedInstaller, as OpenSSH for Windows checks a key, and a file with no DACL is open to all. The check reads the open handle. Who else may open a file comes back as human text: `mode 640` on Unix, account names on Windows.
- **`DurableName`** gives a file its name, replacing the file of that name, so the name survives a crash: a rename then a directory sync on Unix; `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` and `MOVEFILE_WRITE_THROUGH` on Windows. It syncs a directory that changed: `fsync` on Unix, `FlushFileBuffers` on a directory handle opened with `FILE_FLAG_BACKUP_SEMANTICS` on Windows. A failed rename is never retried ([D9](10-sessions.md#decisions)). It also names a file only where no file holds the name: `link` then `unlink` on Unix, `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` on Windows.
- **`StopSignals`** catches the signals that stop a process by its exit, for the rest of the process: the first raises a flag the app reads each frame, and a second ends the process at once with code 130. On Unix, `SIGINT`, `SIGTERM` and `SIGHUP`, each only when the process did not start with it ignored, which `sigaction` reads; `signal-hook` catches each, with a handler that only sets the flag or calls `_exit`, as POSIX lets a signal's handler. On Windows, the console's Ctrl-C, Ctrl-Break, close, logoff and shutdown, through `SetConsoleCtrlHandler`, whose handler runs on a thread of its own: Windows ends the process once the handler of a close, logoff or shutdown returns, so that handler waits for the process's exit, within the 5 s Windows gives, as .NET's `PosixSignalRegistration` does; a process started in a process group of its own takes no Ctrl-C, as Windows has it.
- **Test helpers** in `store`'s internals expose a file to other users, make a link, check a path, and start a child with the terminal's signals ignored and stop it, so a test of a promise runs on every OS with no `cfg`.
- **Unsafe code.** The module alone holds `unsafe`: Win32 through Microsoft's `windows` crate, and `libc` for the effective user and a signal's disposition on Unix, and in the test helpers for a child's signals. Each file allows `unsafe_code` with its reason, and each block says why it is sound.
- **The same result on every OS** where no code differs: a package path names only a file every OS holds as the same ([Package paths](03-game-scripting.md#game-package)); each walk of a directory sorts its entries first; a process's end is std's `ExitStatus`.
- **Enforced.** A test of `common` scans every source file and fails on an OS `cfg` or `std::os` path outside the module; only `store`'s manifest names a Windows dependency.

## Scripting

`script` is its own crate in the deterministic core, so `sim` is testable without Rhai. Scripts run inside the sim tick and call only the registry's API ([Script API](08-script-api.md)); they never touch the ECS. Not `bevy_mod_scripting`, which exposes all of Bevy and [pins Bevy patch versions](https://lib.rs/crates/bevy_mod_scripting_script).

- **Engine:** `Engine::new_raw` with only the packages scripts need, strict variables ([Engine enums](08-script-api.md#engine-enums)); no `eval`, imports, floats or time; `print` to debug logs only; a fixed hashing seed; never `unchecked`.
- **Operation limits:** a limit per call, and per-tick pools the manifest sets; counts are the same everywhere, so an over-budget script fails the same everywhere. A property read through a type's indexer costs one operation, as a getter does: the host gives each such type a getter and setter of each property its scripts use. A method of a value or a handle takes it as a copy, so Rhai feeds nothing back through a setter. Each hook runs in one pool:

  | Pool | Hooks |
  | --- | --- |
  | The player's, one per slot | The calls the player causes: its units' actions, their deliveries and modifiers, its quests and dialogue |
  | `think` | AI `on_think` and `on_heard`; an action's or modifier's hooks whose source no player controls, or is gone |
  | `mode` | The mode's hooks: timers, inputs, deaths, region events, `on_level_up`, `on_wake`; a modifier with no source |
  | `systems` | `on_system` |
  | None, the limit per call only | `calc_damage` and `calc_heal`, pure, once for each damage and heal, so a crowded fight cannot change how they weigh ([Combat](04-capabilities/combat.md#damage-and-heals)); `on_generate`, under its own larger limit |

  A call that finds its pool spent waits and runs first in the next tick; a pure hook has no pool.
- **Other limits** (call depth, sizes) are engine constants, as Rhai's defaults differ between debug and release.
- **All or nothing per call.** State writes go to an overlay the call reads back; effects are queued, each capability's types in its own queues in the call's frame, each applying itself, so the runtime names no capability. On success the overlay commits, then the effects apply in call order; on failure both are dropped, a `script_error` event is recorded, and the tick goes on.
- **No hidden script state:** it is declared and stored in sim components ([Script state](03-game-scripting.md#script-state)).
- **Measured** (one core of a Ryzen 7 6800U): a hook call costs 63.2 ns with an empty body, `atomic/script/call`, and 384 ns with one call to a registered function, `atomic/script/native`, so each such call, as each `ctx` query, costs about 321 ns. The AI's `on_think` takes 43 % of a 3v3 tick. A batch reads the script view first: 72 ns a unit when every unit moved, `integration/script_view/read`, 33 ns when one in a hundred changed, `integration/script_view/reread`.
- **The script view keeps what did not change.** A read fills again only the parts of a row whose sources changed since the last read, and copies the rest, run by run. Each source finds its changed units through a query of `Changed` filters derived from its parts' type, so it cannot read a part whose change the view misses; a part that comes or goes changes the unit's archetype, which the view compares. A change of what a column derives from besides the units' parts, as the stats column's life pool, of the relations, of a book shared at load, or Bevy's check of the change ticks, fills every row. A debug build reads a second time with every row filled and fails when the reads differ.

## Backends

Collision, pathfinding, movement and visibility each have one interface and pluggable backends, chosen per mode ([Navigation](04-capabilities/navigation.md), [Vision](04-capabilities/vision.md)). Movement, how a unit follows its route each tick, defaults to local steering, and an imported game's own movement is a backend of it. Collision works within each layer: circles and boxes on a plane now; static 3D level geometry or `physics` later. A physics backend may use strictly deterministic floating point inside itself, as Rapier's [`enhanced-determinism`](https://rapier.rs/docs/user_guides/rust/determinism/), pinned per release and checked by the goldens on every OS; everything else is fixed-point, and a NaN panics before it reaches a snapshot.

## Bevy

- `sim` depends on `bevy_ecs` only and is one schedule on one thread: a tick's systems are too small to share, and Bevy's parallel executor made a 3v3 tick cost 2.5 times as much. The server and client run it inside Lightyear's fixed tick; the verifier and the tests in a bare `World`. The server never links the renderer. Pinned to [Bevy 0.19](https://bevy.org/news/bevy-0-19/); the script API and the protocol expose no Bevy types. Sim systems touch only sim components, so Lightyear's cannot change a result.
- **A match is a world the release built.** `Release::build(world, spec)` is the one build: a `MatchSpec` gives the seed, the tick rate, the capabilities and the scripts' budgets, and the build prepares the world, installs the core and each capability, and puts in it the tick's schedule, its built order checked there, the `StateRegistry` and the `CapabilitySet`, so the world holds the match whole. `Release::install_mode` adds a mode once its books are in. Who owns the world is the host's: on the server and the client the `App`, as Lightyear replicates only its main world, and the server's lobby builds the match in it when the lobby fills; everywhere else campfire's `Match`, which owns its world and derefs to it, as the verifier's replay, a restore target, an arena, the benches and a capability's tests hold it: a test builds one with `Match::test` and places its fixtures into it; a session's comes from `Session::start_match`, and the harness's from `MatchBuild`. The match's operations are traits on `World`, so they read the same in every host: `SimWorld` (a stable id's entity, `step`, the state's hash, snapshot, restore and changes), `SessionWorld` (a session's start, ticks, checkpoints and result), and each module's own, `PlayersWorld`.
- A world that runs ticks with no `App`, a replay, a verification, an abort, a restore before its first frame, an arena, a restore target and a test's `Match`, clears its change trackers after each tick, `SimWorld::step` or `SessionWorld::run_tick_alone`, as an `App`'s update does each frame, so a long replay holds removals in bounded memory. The sim tracks its own changes and reads no removed-component message, so its state is the same either way; the proving match checks it.
- The server records the inputs received since the last tick, then runs the tick. It hashes the state at checkpoints and the result; a hash after every tick is opt-in.
- Each unit replicates to the clients whose vision group sees it, and each kind of state to its audience ([Replication audiences](14-replication-audiences.md)). A client predicts only what its player controls, with no input delay: Lightyear keeps its tick ahead of the server's by half the round trip, a jitter margin, a sync margin and one tick. A rollback reruns the sim from the server's state. It predicts movement, never a random outcome. The client builds the same books as the server from the packages it holds, at the rate the listing names, and installs the part of the mode no script runs: the map's metric, bounds, relations, pathing grid and combat's bindings. It derives its units' stats, tags and step, and starts their actions through the core's checks; it runs none of their effects and no script. It drops each sent input older than the deepest rollback.
- `client` draws each unit with its own entity, interpolated between ticks; floats (`Transform`) exist only there.
- **Measured** (one core of a Ryzen 7 6800U): a packet's signature costs 17.5 µs to sign and 25.6 µs to check, `atomic/chain_head/sign` and `check`; a 3v3 tick at 20 Hz costs 114.4 µs on average and 0.83 ms at worst, `integration/server_tick/mean_3v3` and `worst_3v3`, and the first, which builds the schedule and the pathing grid's labels, 1.64 ms, `first_3v3`. A re-simulated tick costs 9.4 µs in the lane 1v1 and 23.0 µs in the 3v3, `integration/client_frame/walk_rollback` and `walk_rollback_3v3` against their runs with no rollback, so an 8-tick rollback at a 200 ms round trip costs about 0.18 ms.

**Determinism rules for `sim`** (numbers, RNG and the state hash: [Determinism Core](09-determinism-core.md)):

- One fixed-tick schedule in ordered stages. A pass over what a stage changed runs in its `SimEdge::After`, one over what changed between ticks in `SimEdge::Start`; a test fails a system of the proving match's schedule in no stage and no edge. Two systems with conflicting access and no order fail the build, and so does a redundant edge, which Bevy only warns of. A test builds the schedule with no automatic sync points, so every order the sim relies on is stated.
- Stable entity ids, never Bevy `Entity`, for the protocol, replays and every order that matters, as Bevy [does not guarantee query order](https://docs.rs/bevy_rand/latest/bevy_rand/tutorial/ch02_basic_usage/index.html).
- Positions are 3D and fixed-point, 1 unit = 1 m, within ±2²⁰ m. No floats outside a physics backend, no randomly seeded hash maps, no wall clock. An overflow panics in every build.
- The RNG is a cryptographic PRF, so clients cannot recover the seed, and a random value that decides an outcome never reaches a client before the log is published.

## Creator tools

- **Data schemas.** A JSON Schema for every data file, generated from the types the engine reads, with a test that fails when the checked-in schemas differ, so an editor completes and checks a package's TOML. The schema crate is chosen and approved when the work starts.
- **Hot reload.** A local session in dev mode reloads changed scripts, data and text without a restart, as Roblox Studio does. A reload is no input a log can replay, so a dev session's terms say so, the verifier refuses its log, and it takes no payments.

## Testing and diagnostics

- **Match scenarios** run whole matches between scripted players (`OrderScript`) through a modeled link of delay, jitter and loss on a manual clock, so each run repeats. Lightyear iterates matched links in parallel, so with `multi_threaded` the links lie in the server's tables in another order each run; the server takes joins by main key and inputs by slot, and a match that plays the same with the links reversed, `LinkLayout::Reversed`, proves it. The harness pins Lightyear's measured round trip to the model's worst, twice its delay and jitter, and its jitter to zero, so Lightyear's lead covers every modeled delay; its frame costs under delay are not real ones.
- **Goldens.** Two pinned records of whole matches: the state golden, each tick's state hash, which moves with the state's layout; and the behaviour golden, a digest of each tick's units by stable id (position, team, owner, pools by id, death), the entities of named ids aside, the tick's deaths with their killers and assisters, the script failures, and each player's resources, which does not. A change of layout alone moves only the state golden. They run on the lane match; on the 3v3, scripted players in one match of a skirmish, a camp whose monster resets, and the farmed lanes; and on the proving match, a mode in `packages/test` that uses every capability the release runs. A scripted player's orders name units by role, resolved to stable ids as each is sent. The 3v3's late rules, which no short match reaches, have a rules test with no golden.
- **One scripted match for each mode.** A new rule of a mode joins its scripted match, its assertion on its event in its own tick; a new run is only for what no replayed input can make. The state is hashed once a tick, which the copy check, the trail and the golden share. A mode's scripted match is the one test exempt from the 1 s bound: the 3v3's, `a_3v3_match_replays_to_the_same_hashes`, plays 14,480 ticks in 6.2 s on an M5. The suite keeps 30 s.
- **Test runner.** Tests run through [cargo-nextest](https://nexte.st), a process per test, as libtest cannot end a stuck test. `source/.config/nextest.toml` marks a test SLOW past 1 s, a bound for a test alone, `--test-threads 1`, and ends it at 5 s, a scripted match, in a module named `scripted_match`, at 1 min. CI's profile ends a test at 30 s and a scripted match at 3 min, as its runners are 3.5 to 6 times slower.
- **Allocation audits.** A crate's `tests/alloc` binary installs `common`'s `CountingAllocator`, which counts each thread's allocations inside a window that thread opened, and keeps the first eight backtraces, resolved only when a frame fails, the workspace's frames alone. Each fixture drives one hot path a frame at a time through `AllocAudit`: a warmup, 64 measured frames, and a budget for each frame, zero unless the fixture's comment names each allocation it allows. A count is its thread's own, so a fixture drives the work it measures on the test's thread, and keeps other work, a peer's side or its own setup, out of the count with `CountingAllocator::excluded`. Each run repeats: a fixture whose library draws random numbers that change what it allocates, as `quinn-proto`'s key updates and skipped packet numbers do, runs on a manual clock with seeded generators. The binary needs `internals`, which the check chain's `--all-features` turns on.
- **Parity.** An imported game's replays play in the original and in Campfire, and a test compares each frame within the game's measure ([Zero Hour](12-zero-hour.md#parity)).
- **Structure tests:** the layer test; the archetype-shuffle test, which moves every unit to a new archetype before each tick, from the highest stable id down, and checks both goldens; the state table test.
- **Work record:** at each stage of a redesign, the instruction count of the proving match and the 3v3, and the worst tick against the mean; a stage that makes either worse by more than 10 % says why.
- **LAN check** (`campfire-lan-check`, on request): the real server, two `client --bot` processes on `127.0.0.1`, and a bot that pins the wrong TLS key, which must fail and say why, checked from their JSON logs and by the verifier. Every order takes effect in its stamp tick, or in the tick after a frame that caught up with a stall, if its stamp is among that frame's ticks ([Sessions](10-sessions.md#decisions), D7). One cargo invocation builds the server, the client and the verifier, so the server and the verifier carry the client's Bevy features, as one build costs CI less. The processes log, beyond their own files, each frame's start with its tick, and each order's send and receipt by slot and chain seq, added through `CAMPFIRE_LOG_FILTER_EXTRA`, so a late order shows which process waited.
- **Zero Hour check** (on request): imports the install `GENERALSZH_DATA` names, fails when it names none, logs the package fingerprints another OS must match, checks a map, and plays a tick ([Zero Hour](12-zero-hour.md#decisions), D7).
- **CI** runs the check chain, each bench case once, and the LAN check on Linux, Windows and macOS; each platform's verifier replays every platform's LAN check logs.
- **Warnings fail tests.** Each fixture that runs a match holds a `LogCheck` (a test's `Match`, `InProcessMatch`, `FixedMatch`, and the arena and the restore target, whose `Match` holds it): a subscriber for the test's thread that keeps each event at Warn or Error. A test that causes one takes it, typed, with `take::<E>()`; when the last check drops, each event no test took fails the test, as rustc's tests and pytest's `-W error` do.
- **Logging** goes through `tracing`; `--help` and `--version` print. `sim` and `capabilities` log nothing: they report through resources the runner logs. Binaries log to standard error, and to JSON lines with `CAMPFIRE_LOG`, filtered by `CAMPFIRE_LOG_FILTER` and `CAMPFIRE_LOG_FILTER_EXTRA`. An event a tool reads back is a typed `LogEvent` with a round-trip test, as is each warning and error `net` and `runner` log.

## Benches

- **One target for each crate.** A crate with benches has one `[[bench]]`, `benches/<module>.rs`, whose `criterion_group!` takes `bench::run`, from `lib.rs`'s `bench` facade. Criterion's filter, a regular expression over each case's `<kind>/<group>/<case>` id, `--exact` and `--list` choose the cases: `cargo benches -- --exact atomic/num/mul`, `-- atomic/num/`, `-- '^integration/'`, `--bench <crate>`. Each library sets `bench = false`, as its libtest harness refuses criterion's flags.
- **One selection for every run.** `cargo benches`, an alias in `source/.cargo/config.toml`, is `cargo bench --features bench` over exactly the crates with a `[[bench]]`, so a case builds from the same features whichever cases run. `--workspace` would unify the client's Bevy features into them, the render stack on the critical path and `multi_threaded`; one `-p` loses features others turn on, so a case would measure other code. A crate that gains a `[[bench]]` joins the alias.
- **Fixtures.** A fixture that loads packages or runs ticks is made in the first case that needs it, a `LazyCell` for one cases only read, an `Option` filled by `get_or_insert_with` for one a case changes, so a filtered run costs nothing. A fixture of plain values reads no file and runs no tick.
- **Two kinds.** An **atomic** case runs one operation over 4,096 inputs drawn from `split_mix`, or one input when it costs more than a microsecond. An **integration** case runs a system for a tick on a made scene of 1,000 units, the scale of a large Zero Hour battle, from `KernelScene`, crowded into 40 m square and spread over 120 m square where density changes its cost; or runs one end on the test packages: the server's tick or frame, or the client's frame, by `iter_custom`.
- **A case's id.** `<kind>/<group>/<case>`, the group the path measured, unique in the workspace. Atomic: `num`, `floor_root`, `ceil_root`, `rng`, `vec3`, `u256`, `product_sum`, `rounding`, `body_box`, `shape`, `grid`, `body_grid`, `body_index`, `chain_head`, `script`. Integration: `collision`, `pathing_grid`, `route_planner`, `gather`, `fog`, `battle`, `script_view`, `state_hash`, `snapshot`, `state_delta`, `session_log`, `package_load`, `match_build`, `script_compile`, `script_bind`, `server_tick`, `server_stage`, `server_frame`, `client_frame`. The case is the operation, the scene (`crowded`, `spread`), or the statistic and scenario (`mean_3v3`, `worst_3v3`); an end has a `mean` and a `worst` case, as the worst tick is the budget ([Tick rate](04-capabilities/00-overview.md#tick-rate)). Each case states its `Throughput::Elements`, and its doc comment names the path, the scale and the cost unit.
- **A stage case times one stage of the real tick.** `sim`'s `StageClock::install` adds a probe at each of the ten edges around the nine stages, writing time into a resource outside the state, so the hashes stay the same; the proving match plays to both goldens with the probes in.
- **A system's case for each path past 10 % of a tick.** Every stage has a case in `server_stage`; a path past 10 % of the mean or worst 3v3 tick also has a system's case: `think` in `script`, `vision` in `fog`, `resolve` in `script_view`, `inputs` in `pathing_grid`, `move` in `route_planner`; `battle` times each stage at a large battle's scale. A capability's Cost section names its case, the figure and the machine.
- **Profiling.** Build with frame pointers in its own target directory: `RUSTFLAGS="-C force-frame-pointers=yes" cargo benches --bench <crate> --no-run --target-dir target/frame-pointers`, then `perf record --call-graph fp` on the bench binary with `--bench <id> --profile-time 10`. DWARF unwinding loses the optimized stacks, and AMD has no LBR.
- **In CI.** Each case runs once on every platform, `cargo nextest run --workspace --all-features --profile ci --bench '*'`, a process for each case, so a panic, a broken `debug_assert` or a repeated name fails the run. A case has its own limit of 60 s.

## Networking

The open protocol is separate from the transport:

- **`protocol`** (versioned, documented): the session log and everything a replay needs.
- **`net`**: [Lightyear](https://github.com/cBournhonesque/lightyear) for transport, replication, prediction with rollback, interpolation and interest management, through [`bevy_replicon`](https://github.com/simgine/bevy_replicon), whose per-client visibility the vision rooms and the kind filters drive ([Replication audiences](14-replication-audiences.md)). `net` depends on `bevy_replicon`, pinned to Lightyear's version, for the filter Lightyear does not re-export. The server and the client link over QUIC alone, through `quic`, an IO layer under Lightyear's link with one thread for each endpoint and no async runtime; the harness and the local server link by in-process channels ([QUIC transport](15-quic-transport.md)). Client and server always run the same engine release. Lightyear 0.30.1 is patched in `source/Cargo.toml`'s `[patch.crates-io]` until upstream fixes it: it kept the state at its history's cut as an authoritative sample, so a client that ran further ahead than the rollback window, as after a server stall, snapped every later rollback onto a stale value; its raw server named each link by its address, which two QUIC connections can share ([QUIC transport](15-quic-transport.md), D9); its aeronet link kept an MTU past its session's; and a received payload became mutable, a copy when its allocation was shared, where the transport freezes it at once. `bevy_replicon` 0.44.2 is patched there too, until upstream takes the fix: in the tick a filter hid a component from a client, it sent that client the entity's mutations apart from the removal, and the client dropped them as older than the removal's update ([Replication audiences](14-replication-audiences.md), D17).

## Libraries

Exact versions are pinned across the workspace. Each release tag pins its Rust toolchain and `Cargo.lock`, so anyone can rebuild a tag and compare hashes of the unsigned build outputs with the published binaries. Release trust follows [TUF](https://theupdateframework.io/docs/security/): offline keys, a threshold, revocation.

| Area | Crate | Notes |
| --- | --- | --- |
| Fixed-point numbers, trig, roots | Own code in `math` | Why not `fixed`, and the rounding and error bounds: [Determinism Core](09-determinism-core.md) |
| SIMD lanes | Own code in `math` | `I64x4`, `U64x4`, `Mask64x4`, as array loops LLVM vectorizes on x86-64-v3 and armv8-a, exact as the scalar code. Only ops with a vector form on both: no 64×64 multiply, division only through `f64`, exact within 2⁵¹. An op wraps, as a checked one would not vectorize; its doc states its domain, a debug build asserts it, and a caller checks its inputs once and takes its scalar path outside. A caller stays only when it measures faster on x86-64. `std::simd` is unstable, intrinsics need `unsafe`, and inline assembly measured slower |
| RNG | `blake3` keyed hash, in `math` | A PRF by specification, counter-based as [Random123](https://www.thesalmons.org/john/random123/papers/random123sc11.pdf) but cryptographic; own range sampling. No `rand`, which [may change output in minor releases](https://www.rustmax.net/library/rand-book/crate-reprod) |
| Binary encoding | `postcard` | [Stable wire format](https://postcard.jamesmunns.com) since 1.0, through `codec` |
| Data files, log lines | `toml`, `serde_json` | Through `codec` |
| Fixed buffers | `arrayvec`, in `common` and `sim` | The batches an encode or a hash fills; already in the build through `blake3` |
| State hashes | `blake3` | At checkpoints and the result; every tick in the goldens |
| File and package fingerprints | `sha2` | SHA-256, as Blossom addresses blobs |
| Secret buffers | `zeroize`, in `store`, `protocol`, `server`, `quic` | Already in the build through `rustls` |
| QUIC | `quinn-proto`, `quinn-udp`, `polling`, in `quic` | QUIC with no IO, its batched socket, and the thread's wait; no async runtime ([QUIC transport](15-quic-transport.md), D1 to D3) |
| Stop signals | `signal-hook`, in `store` | A signal's handler that does only what POSIX lets one, on Unix; Windows' console events go through `windows` ([Platform](#platform)) |
| Port mapping | `igd-next`, `netdev`, in `quic` | UPnP-IGD by its blocking API, and each OS's default gateway, to which `quic`'s own PCP and NAT-PMP speak; no async runtime ([QUIC transport](15-quic-transport.md), D12) |
| TLS | `rustls` over `ring`, in `quic` | TLS 1.3 alone, a raw Ed25519 key and a pinned one; `ring` derives the TLS and reset keys by HKDF ([QUIC transport](15-quic-transport.md), D6 to D8) |
| Error types | `thiserror` | `log`'s `ErrorReport` writes the chain |
| Other `Display` impls | `derive_more` | |
| Command lines | `clap` | Each binary's `Args`, values through `FromStr`, exit code 2 for misuse |
| Nostr | `nostr` in `protocol`; `nostr-sdk`, `nostr-connect` for `identity` and `ownership` | Still alpha |
| Lightning (`payments`) | `nwc` | Drives the host's own wallet, including [hold invoices](https://getalby.com/blog/build-conditional-payment-logic-into-your-app); no embedded node |
