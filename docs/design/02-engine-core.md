# Campfire — Engine Core

The engine's structure: its modules, the rules that hold them, and the choices of each engine-wide system. A capability's own mechanism is in [Capabilities](04-capabilities/00-overview.md).

## Modules

| Module | Status | Kind | Does |
| --- | --- | --- | --- |
| `common` | built | problem | The plain values crates that do not depend on each other share, the named enums, the domain hashers and the error report |
| `common-derive` | built | problem | `common`'s derives, which `common` alone names and re-exports: an enum's members, their identifiers and their names |
| `codec` | built | proxy | The one gateway of serialization, a proxy crate ([Serialization](#serialization)) |
| `schedule` | built | proxy | The sim schedules' build settings, a proxy crate |
| `math` | built | problem | Fixed-point numbers and vectors, trig, roots, RNG ([Determinism core](07-determinism-core.md)) |
| `protocol` | built | problem | Signatures, keys, the handshake, input and seed chains, the session log; no IO ([Protocol Spec](05-protocol-spec.md)) |
| `sim` | built | glue | Deterministic state and systems on `bevy_ecs`; no genre code |
| `script` | built | glue | The Rhai host, the script API's registry, and the script view ([Scripting](#scripting)) |
| `capabilities` | built | glue | The top of the capabilities' crates: the release, the books, `Match` |
| `values` | built | glue | Data value types, names and ids |
| `geometry` | built | problem | Exact geometry: metric, bounds, bodies, polygons, grids |
| `state-types` | built | glue | The state types modules declare, and how each is sent |
| `visibility` | built | glue | Who receives each kind of state ([Replication audiences](10-replication-audiences.md)) |
| `declare` | built | glue | How a module declares what it adds to a match |
| `units` | built | glue | The unit model, the map's book and its ground |
| `players` | built | glue | The match's and players' entities, resources and commands |
| `stats` | built | glue | The `stats` capability |
| `actions` | built | glue | The action pipeline and slots |
| `deliveries` | built | glue | Actions' deliveries and their hooks |
| `combat` | built | glue | The `combat` capability |
| `navigation` | built | glue | The `navigation` capability |
| `vision` | built | glue | The `vision` capability |
| `projectiles` | built | glue | The `projectiles` capability |
| `areas` | built | glue | The `areas` capability |
| `abilities` | built | glue | The `abilities` capability |
| `progression` | built | glue | The `progression` capability |
| `production` | built | glue | The `production` capability |
| `interaction` | built | glue | The `interaction` capability |
| `items` | built | glue | The `items` capability |
| `orders` | built | glue | The `orders` capability |
| `mode` | built | glue | The mode: its script, state, choices and spawns |
| `package` | built | glue | A mode's packages, checked against their index; no IO |
| `client-files` | built | glue | The client's files of a package, which the sim never reads |
| `package-disk` | built | glue | A package directory and the package store on disk |
| `runner` | built | glue | Builds and feeds a match; replays a published log |
| `bots` | built | glue | The bots' order scripts |
| `verifier` | built | app | CLI that replays a session log and checks it |
| `lan-check` | built | check | The real server and bot clients over QUIC on localhost, on request |
| `zero-hour-check` | built | check | Imports and checks a Zero Hour install, on request |
| `vocabulary` | built | check | Writes the repository's layout and the names its Rust files declare to one text file for review, on request |
| `harness` | built | test | The session and network tests' match harness; never shipped |
| `rules` | built | test | The workspace's structural rules as tests; never shipped |
| `test-packages` | built | test | The workspace's test packages; never shipped |
| `measure` | built | test | What the tests and benches measure with: the counting allocator and the allocation audit, criterion's measure of allocations, and every bench's kinds and sampling; never shipped |
| `server` | built | app | The dedicated server's binary |
| `net` | built | glue | Lightyear over `quic`, and the session host ([Networking](#networking)) |
| `quic` | built | glue | QUIC under Lightyear's link ([QUIC transport](11-quic-transport.md)) |
| `net-common` | built | problem | What several of the network stack's crates share: a writer's chunk arena, a buffer of ready items, seeded randomness and the benches' scheme |
| `net-core` | built | problem | The network stack's vocabulary, with no engine in it: the app's instant, a link's direction, network ticks, peer and network ids |
| `net-bytes` | built | problem | The network stack's byte buffers: a writer that reuses its chunks, a reader, varints, and `ToBytes` for the stack's own headers |
| `net-timeline` | built | glue | The app's network clock and its timelines |
| `net-payload` | built | problem | The queues between the frame and each link's packet session, the payloads in them, and why a link ends |
| `net-emulator` | built | problem | The link model and the emulated network the stack's tests and singleplayer run on |
| `net-link` | built | glue | A link's state and its lifecycle, with no transport in it |
| `net-connection` | built | glue | A client's and a server's connection on a link, and an app's networking topology |
| `net-packets` | built | problem | Each link's packet session: numbered, acknowledged and, on a network, sealed packets |
| `net-udp` | built | io | The stack's own UDP protocol: sealed sessions by a TLS 1.3 handshake in its own packets |
| `net-endpoint` | built | glue | Each IO's endpoints as components, and the frame's steps between the link queues and the sessions |
| `net-stepper` | built | test | The stack's test stepper: a server and clients on the emulated network on one virtual time; never shipped |
| `net-channels` | built | problem | Messages over a link's packets: channels and their delivery, fragments, priority and compression |
| `net-transport` | built | glue | A link's channels on its entity, the channel registry, and the frame's receive and send steps |
| `net-messages` | built | glue | Messages and events an app registers, sent and received over the channels |
| `net-sync` | built | glue | Clock sync between peers: the latency estimate, the remote timeline and the local timeline's controller |
| `net-despawn` | built | proxy | The one despawn of the stack's replication crates, a proxy crate: it leaves each entity the server still owns to the server |
| `net-replication` | built | glue | Entity replication on `bevy_replicon`: what each peer receives, who controls an entity, and its history |
| `net-interpolation` | built | glue | The interpolation of replicated entities between their server ticks |
| `net-prediction` | built | glue | The client's prediction of the entities it controls, its rollback and its correction |
| `net-port-map` | built | io | Asks a home router to forward a server's port: PCP, NAT-PMP, then UPnP-IGD |
| `net-stack` | built | glue | The plugin groups that install the network stack's crates in the order they need |
| `log` | built | problem | Log output and the events a tool reads back |
| `store` | built | proxy | Files, data directories, threads and the platform layer, a proxy crate ([Storage](#storage)) |
| `launcher` | planned | — | Fetches, checks and starts the engine release a server names |
| `client` | built | app | The Bevy client |
| `import` | built | app | Writes one reproducible package from a game's install ([Zero Hour](09-zero-hour.md)) |
| `content` | planned | — | Package signatures, pinning, Blossom fetch |
| `editor` | planned | — | Map and content editors |
| `identity` | planned | — | Nostr keys, listings, reputation |
| `ownership` | planned | — | License checks |
| `payments` | planned | — | Pools, hold invoices, wallet budgets |

- **A crate for each module.** Each module is a crate, `campfire-<module>`, as Bevy's are, so Cargo checks every import against the graph. A test holds the table above equal to the workspace.
- **Layers.** Each crate depends only on lower layers, and no two crates of one layer depend on each other; a capability crate's layer is one above its highest dependency's, read from the manifests, so no table restates the graph, and one reader of the manifests serves every rule. Refused: a hand-kept list of layers, which can only agree with what Cargo already allows, and `cargo metadata`, which a test would spawn to read the graph the manifests already give.
- **Every crate declares its kind.** A manifest names its crate's kind under `[package.metadata.campfire]`: `problem`, one problem with no engine in it, as a protocol, a codec, a model or an algorithm; `io`, a problem crate that holds a network IO; `proxy`, a proxy of the bans; `glue`, what binds problem crates to the ECS or holds a problem whose nouns are entities and components; `test`, what only tests build; `check`, a binary that checks the engine and never ships; or `app`, a binary that ships. A problem or IO crate has no engine crate in its required closure, so it builds and tests alone; a glue crate requires no IO crate, which reaches the ECS through the network stack's facade alone; and a library requires no test crate; a `rules` test holds each, and the table above gives each crate's kind. Refused: each layer one crate with its ECS code inside, which compiles the engine into every protocol and codec test; the kinds of the network stack alone, which left `values` and `visibility` on the ECS for six files; a hand list in `rules`, which missed a problem crate. Source: `quinn-proto` holds QUIC with no IO and no OS time and `quinn` binds it to Tokio; `renet` and `bevy_renet`; rust-analyzer, where one crate alone knows LSP.
- **`bevy_platform` and `bevy_math` are engine-free.** A problem crate may hold them: `bevy_platform` gives the clock Bevy's `Time` reads and the fixed-hash map, and `bevy_math` vectors, neither with an ECS or an app. Refused: Bevy's math counted as engine, which keeps a crate of plain vectors glue.
- **A plain type the ECS holds takes its ECS derives behind its crate's features,** which the glue crates turn on: `Reflect` behind `reflect`, and `Component` or `Resource` behind `ecs`. So one type keeps one name in both kinds of crate. Refused: dropping the derives, which the ECS's resources and components need; a remote reflection or a newtype in the glue, which repeats each type's fields or its name.
- **The sim reads no file and logs nothing.** No crate a match runs on requires `store`, `log` or `tracing`, and `protocol` does not require `store`, so a match runs the same on a server, a client and a verifier, and a tick waits for no disk; a `rules` test walks the release's required dependencies.
- **A script's print is a report.** In a debug build, `print` and `debug` record into the tick's `ScriptPrints`, which the server logs after the tick as it logs the tick's deaths; a release build drops them, so an untrusted script cannot fill a server's log. Refused: a call to `tracing` in the host, which made the script crate the one crate of a match that logs, and an ageing message buffer, which serves readers within the sim, as the removals do, and the sim reads no print.
- **A pure sim.** `sim` takes state and inputs and gives the next state: no files, packages or signatures. No crate of the deterministic core depends on `store`, and `store` depends on no engine crate.
- **`common` holds only shared plain values.** A type enters `common` only when two crates that do not depend on each other both name it, with construction, parsing, display and serde alone.
- **A name stays beside its member.** An enum derives `NamedEnum`, each member named by its identifier in snake case, as serde's `rename_all` names it, or by its own `#[name = "…"]`, and data and serde read it by those names alone. Not strum, whose names and serde's are two attributes that can drift; not a declarative macro, whose body rustfmt and the IDE read as opaque.
- **Needs and uses.** A capability's dependency on another is a need, without which it does not run, or a use, which it reads when the match has it, as `Module::NEEDS` and `Module::USES` declare; a test holds the crates' dependencies equal to them.
- **The senders' `Order` lives at the top.** `Order` and its `Action`, a sum of each capability's action, are the top crate's, so `orders` depends on no capability whose action it carries.
- **Contracts through `units`.** A capability asks another for work through a part of `units` both read, never by writing the other's state, as 0 A.D.'s components ask `UnitMotion` for a walk: `Dead`, `ForcedMove`, `Destination` with `ArrivedShort`, combat's record of the tick's `Deaths` and vision's `SeenBy`, which the server's sending reads with no capability crate. A change the asker decides and the owner applies is an event of `units` the owner observes, as `LevelRaised`.
- **One published surface.** A crate's `lib.rs` names only its own items, and a caller depends on each type's home crate.
- **One rule of test homes.** A test lives in the lowest crate whose dependencies hold everything it uses: one that builds a whole `Match` in the top crate's `tests/`, one that plays a session or a network match in `harness`, and one of a layer of the network stack in the lowest crate whose stack has every layer it uses. A dev-dependency never builds on the crate that takes it, so no test build holds a second copy of a crate, whose types would not be the match's; test support, as the test packages, may stand above a crate whose tests read it. Refused: a test home rule for the stack apart from the engine's; a ban on every dev-dependency above a crate, which moved a test that reads a module's private items away from them.
- **The core's crates in order.** `script`, `declare`, `units`, `players`, each on its own layer: the script view is the runtime's own data, which the modules fill, so `units` depends on the runtime and never the other way.
- **Game-bound code.** Code that serves one imported game lives in a module named after it, in the crate whose interface it serves, as `import::zero_hour`; nothing depends on it but the place that installs it, and a mechanism a second game needs moves into a general module.
- **One rendering pipeline.** The client draws every game with Bevy's physically based standard material and the generic formats, glTF and the client files; an importer converts its game to them and writes down what it loses, and a need the pipeline cannot say becomes a general feature, never a path for one game.
- **Bots are inputs.** A bot plays an order script of `bots` and produces inputs as a player does, so no replay depends on bot code.
- **Books are one pure function.** One function of a match's packages and a tick rate builds every book, with no world. The load calls it at the fastest rate the manifest allows, where a time counts the most ticks, so what a book cannot hold fails the load; ids follow the builder's order.
- **Books built by their modules.** A module declares each book it reads with its builder, and the build makes a book the first time a builder asks for it, so a module reads the books of the modules it builds on whatever the order of the list, as Bevy's asset loaders load what an asset depends on; only the modules of the match build. A fact is what a lower module's book reads of a higher module's data, as actions read the damage kinds of combat's `[combat]`, and an extension adds a higher module's part to a lower module's book, as an inventory adds its slots to the action slots; an extension of a book of a module the match does not run does nothing, as actions add to the stats' graph in a match of no stats, and one of a book no module of the release declares is a logic error. A module's error is its own type, which the book error carries for the load to read by type. Refused: one builder in the top crate that named every capability's books, sections and names, and built every book for every mode.
- **Books before the install.** A match's books are built before its build, and each goes in as its module declares it, before that module installs, so an install reads the facts of its books and adds only the systems they need, as Bevy's plugins add systems by their config at build; a match installs each module once, its mode with it, and builds its schedule once. Refused: an install of empty books that the books filled after, which made each load fact a run condition of every tick and the mode's install a second build of the schedule.
- **A unit spawns in one write.** A spawn puts the kind's marker, the caller's parts, the core and every declared part on the entity in one move between archetypes, the caller's parts taken by their components, and a declared part is made from the core, its facing angle among it, and the type's books, never from the half-built entity. Refused: the caller's parts first and the declared ones after, two moves for each unit, as Bevy inserts a bundle only of types fixed at compile time; a harness watch counts each spawn's moves in the 3v3, the lane match and the proving match.
- **Spawn reactions run on the spawner's path.** The capabilities' spawn callbacks, a type's passive and a box's room, run where the mode's spawner spawns a unit that stands, and the load refuses a passive on a delivery type, so no reaction is lost. Refused: observers of a unit's type, which would fire at a restore and at a client's replication too, and so apply a passive the snapshot holds again or move walkers the server placed.
- **A unit type's section says which types hold it.** A section of a unit type is a stander's, a delivery's, which makes its type a delivery type, or one any type holds, as the tags, so the units' rule that a delivery type holds one delivery's section and only those of any type names no capability's section. Refused: a list in the load check of the sections a delivery type may hold, which named the sight of `vision`.
- **A module checks the rules of its own data.** Each rule of the data a module reads is a rule the module declares, which the package load checks before its own checks and before it builds the books, so a script that names what a broken rule leaves out shows the rule, and whose error is the module's own, which the load shows at its place; `package` keeps only the checks across packages: names, scripts, texts and the index, of what each module adds to a fact of the core that its data names, a script with the role it serves or a message. A build of data the load checked, or a test's, checks no rule. Refused: one load check in `package` of every capability's rules, which named each module's data and errors, so a new rule edited the session layer.
- **A test's match installs at its first tick.** A test builds its match with its declarations and its books in place and loads its own data into the books; its first tick seals its unit types, adds the systems as the books then are, and builds the schedule, and a book that loads after it panics, so a fact an install read cannot change under it. Refused: a run condition or a pass of nothing to do for each fact a test may load late, which a match of packages pays each tick, and a fixture that writes each test's data as package files first, which would rewrite every test's loads; Bevy's `Plugin::finish` runs the same way, after every plugin's build and before the first update.
- **A table's sections are its modules'.** Each key of the mode's table, of a map's, of a package's content and of a unit type's that a module reads is a section of the module's own type, which the module registers by its key, and the release reads every table with the sections of all its modules, as Bevy's type registry reads a scene's components by their type paths; the load refuses a section of a module its mode does not run. A table reads through a serde seed as the file's reader walks its keys, so an error keeps its place and its message. An open set a section holds, as an action's effects, reads the same way: each member is a module's type that it registers by its key, and a section's read takes the release's readers of those sets. Refused: one closed struct for each table in the top crate, which named every capability's sections, and a read of each table into a value first, which lost the place of an error.
- **The match's names are a plain resource.** Every book shares its names into `MatchNames` as it installs, and the mode its own as it loads, so a check or a restore reads them off the scripts' thread; the script view derives its scripts' strings from it as a batch reads, when its revision changed. Refused: names owned by the non-send view, which pinned every check that reads a name to the scripts' thread; the names written into both, which only a hand keeps in step; and Bevy's change tick, which misses a second change within one world tick, as a test's loads make.
- **Only a script handle is non-send.** The view, the `ctx`, the script host and what holds their values are non-send, as Rhai's values are; every other resource is `Send`, its script values split into what the script's call extension holds, as the mode's params and `ctx.map` are, so a system or a check reads it as a plain resource. Refused: a non-send resource for data that holds no script value, which pins its readers to the match's thread for nothing.
- **Each declaration installs by role.** A state resource, a scratch, a report and a script view column go only into a role that uses them, so a client system that takes the server's state fails its parameter validation, rather than read a value no server sends. The match's facts, its capabilities, its role and its view of a player out of play, are one resource, which a module of the list declares with the sim's own resources, so the audit of resources reads no hand list.
- **Sets order against the steps of the modules they build on.** A module set runs in a stage, or at a step of a module its crate builds on, and after or before such steps; the steps of a pipeline are the sets of the module that owns it, as `actions` holds the action pipeline's and `units` the tick's begin and end, and Bevy's plugins order against `TransformSystems::Propagate`. Two modules that know nothing of each other meet only at the steps of one they both build on, and the strict schedule refuses a pair of their systems no step orders. Refused: one list of every step in `declare`, which named each capability's steps in the core under them.
- **A system's set is its module's by type.** A module names its sets as its `Set` type, which the declarations walk declares, and its install adds each system through `InstallCx<'_, Set>`, which takes one of them, so no system sits in another module's set or in none. Refused: `in_set` by hand at each system, which a test had to catch afterwards.
- **Progress apart from configuration.** A value that changes as a unit works, as a site's progress or the cursor of a route, is a part of its own beside the part it measures, which stays as it was set, so a step of the progress marks, sends and hashes only the progress. Parts that stay in step by index change through one query of them all, as `SlotsMut` holds a unit's slots, their times and its progress, and each pairing of them asserts one entry of each. Refused: one part of both, whose every step resent and rehashed the whole, as each waypoint passed resent the route's waypoints.
- **Markers stay in tables.** The markers that toggle during play, `ForcedMove`, `Resetting`, `Kept`, `Respawn`, `AreaTrigger`, `OutOfPlay` and `HeldOut`, keep Bevy's table storage, as sparse-set storage of each moved neither the battle benches nor the whole 3v3 tick past the spread of a run ([Sparse-set markers](../research/sparse-set-markers.md)); `Dead` stays in a table too, as `Without<Dead>` on the hot queries is dense only for tables. Refused: sparse sets on the ground of the archetype moves alone, which no bench shows to cost.
- **`net` is one crate.** The client's and the server's roles stay in one crate, as Lightyear and bevy_replicon keep both in one: the workspace build unifies their features anyway, and fat LTO drops what a binary does not reach. Refused: a crate for each role.

## Names

The vocabulary the code, the data, the scripts and the documents share, each word with one meaning; the audit that found where they drifted is [Vocabulary](../research/vocabulary.md).

- **An owner is a player.** `owner` is the seat that owns a unit, as the replication's audiences name it; the module that declares a thing is its module, or its `capability`, and the struct that holds a worker its holder. Refused: `owner` for a player, a module, a place in data and a file only its user opens at once.
- **A seat is a player's, a slot a unit's.** `Seat` numbers a session's players, as a lobby seats them, and `slot` alone is a unit's action or item slot, as a MOBA binds its abilities to slots. Refused: `slot` for both, `PlayerSlot` beside `ActionSlot`.
- **A part is a component an entity carries.** A module's share of a call's frame is a call extension, as `http::Extensions` keeps data by type; a value's fields taken apart are its `*Fields`; a model's pieces are its nodes. Refused: `part` for any piece of anything.
- **A place is a position in an ordered whole.** An index in a list, a step of the schedule, an entry of a log or a path in a package's data; a location in the world is a location or a point. Refused: `place` for a world location beside an error's place.
- **A kind is a member of a closed set.** An entity's kind and a damage kind alike; a `type` is a unit type or a script value's type.
- **A hook is a script's.** An engine function the core calls for a module is a callback, as `SpawnCallback`.
- **Lifecycle verbs by what they bracket.** `begin` and `end` a span the caller drives, as a tick or a batch; `start` and `stop` what runs on its own, as a thread, a process or an endpoint's accepting; `open` and `close` a handle; `finish` consumes a builder, as `Hasher::finish` does. Refused: `start_tick` beside `begin_tick`, and `start` beside `begin` on one type for two jobs.
- **Errors by what they report.** `*Error` an expected failure; `*Refusal` why a capability, a peer or a gateway refused a request; `*Refused` the event that one did; `*Problem` what is wrong with data, which an error carries; a test's `Flaw` the break it makes in data. A `*DataError` takes its subject, singular, or the capability's name when it covers several.
- **A predicate names a state or a relation.** A state is `is_<adjective>`, as the Rust API Guidelines' `is_empty`; a relation keeps its verb, as `holds` and `contains`. Refused: a bare adjective beside an `is_` one.
- **Whole words.** `position`, not `pos`, in code, data and scripts; `dir` in a name and "directory" in prose; `len` a count of elements or bytes, `length` a geometric length. `Ctx`, the script's `ctx`, and `Cx`, an engine context, are two words of two meanings.
- **A `*Spec` is a resolved section, a `*Setup` what a thing starts from.** A `*Data` is a section as a package writes it, a `*Spec` the same with its names resolved, a `*Book` a module's specs by id, a `*Decl` what a module declares, a `*Setup` what a match, a mode or a server is built from.
- **A binary's command line.** Its clap struct is `CommandLine`, in its `command_line` module, beside `Args`, what the line resolves to, where the binary resolves it.
- **A group holds crates.** A group's top crate has a directory of its own name beside the group's others, as `sim/sim`, `network/net` and `capabilities/capabilities`, and no crate's directory holds another crate, which a `rules` test holds.
- **Design documents number in reading order, with no gaps.** A document that goes takes its number with it, and the later ones close up; code cites a document by its path and a decision's title, never by a number alone.

## Structural rules

Each rule has a test that fails when it breaks, as a rule only review holds drifts again.

| Rule | Enforced by |
| --- | --- |
| One home for each fact: a package fact in one immutable book, a match fact in state, a call's fact in its frame. | The declaration audit; the behaviour golden |
| Each module declares once all it adds to a match but its systems, and every consumer reads that one list: the install, the script API's binding, the network's lists and the audit. An install holds only its systems, each in a set of its own module; a fact the install or the load fixes is a fact of the schedule, not a check each tick; a set orders against the steps of the modules it builds on alone. | `Declarations` and `ModuleSet` have no default method; the declaration audit; the test that a module names another's set only in its sets' places; the strict schedule's ambiguity check; `Release::unlisted_resources` |
| A name becomes an id where it enters, and every lookup by name is a method ending in `named`; the book builder that resolves a name is its check. | The book builder's tests; the name-lookup allowlist; the load's flaw tests |
| A rule of a package's data has one home, the function that tests it, and untrusted data never reaches an `expect`. | The load's flaw tests |
| The load refuses everything a match can refuse: a start fails only on its terms and on `on_match_start`'s runtime failure. | `StartError` has no case of a package's data |
| A layer calls a higher one only through a hook the higher one registers. | Cargo; the layer test |
| No crate re-exports another crate, whole or in part, but one: `common` re-exports the derives of its own traits from `common-derive`, which no other crate names, as `serde` re-exports `serde_derive`, since a proc-macro crate can export nothing else. | `no_crate_re_exports_another_crate`, `only_common_names_the_crate_of_its_derives` |
| Every order that matters is by stable id: a system that spends something shared, takes ids or runs scripts walks its units through `Ordered`. | The archetype-shuffle test |
| Each tick's work has a fixed limit or a cost in proportion to the units that take part. | The work record; the benches |
| Restored state is checked like package data: every flaw is an error, never a panic; times and counts stay within 2⁶², every period at least a tick. | The `Tick` decode; each state type's check; the snapshot, structured and mutation fuzz |
| Each rule of a network session has one home on each side, and a client that follows the rules is never refused. | The net scenarios under load |
| A function that acts on a match has one home, by what it acts on: the whole match a trait on `World`, the match's content an associated function of its module's type, a value made from the world a constructor of its own type, a test fixture a struct of the ids it places. | The world rule test of `rules` |
| `common` depends on no crate but `serde`, `derive_more`, its hashers and `zeroize`. | The manifest test of `rules` |
| Each crate names its kind: a problem or IO crate has no engine crate in its required closure, a glue crate requires no IO crate, a library requires no test crate, a proxy is a proxy of the bans, and the crates table gives each kind. | `each_crate_keeps_the_rule_of_its_kind`, `the_design_marks_built_exactly_the_crates_of_the_workspace_with_their_kinds` |
| Each crate takes the workspace's lints. | `each_crate_takes_the_workspaces_lints` |
| Each optional dependency is turned on by a feature of its crate, by `dep:` or by a feature of its own, so a build of the crate's features holds it, not only `--all-features`. | `each_optional_dependency_is_turned_on_by_a_feature` |
| A dev-dependency that turns on `internals` of a normal dependency names a crate whose `internals` gates code of its own. | `a_dev_dependency_turns_on_internals_that_gate_code` |
| A required dependency is one production code names; one only `internals` code uses is optional, and one only tests use is a dev-dependency. | `a_required_dependency_is_named_by_production_code`, `an_optional_dependency_is_named_by_library_code`; cargo's `unused_dependencies` |
| No sibling modules of a crate import each other in a cycle; two types that are one table in two parts share a module. | `no_crates_modules_import_each_other_in_a_cycle` |
| A banned call goes through its proxy crate: files and threads through `store`, formats through `codec`, schedule settings through `schedule`, the network stack's replication despawns through `net-despawn`; only a proxy's root exempts the lint, and a proxy that lacks what a caller needs gains it. | `clippy.toml`'s bans; the tests of `rules` on each exemption and proxy |
| Code that differs by OS lives only in `store`'s `platform` module. | The platform rule test |
| A citation of a design decision names a document and a title it holds. | `a_citation_names_a_decision_its_document_holds` |

## Lifecycle and sessions

- **Three engine states.** Waiting, running, ended; what happens inside running, picks and rounds and overtime, is the mode's scripts', and a persistent world never ends.
- **Gates and hooks for optional modules.** Start gates and end hooks are how an optional module joins the lifecycle, so `payments` locks and settles stakes and the engine holds no money code.
- **A session outlives its process.** A match restores by replaying its log from its latest checkpoint, and the server logs each input before its tick and syncs from a background thread, so no tick waits on the disk ([Sessions](08-sessions.md)).
- **Everything outside is a recorded input.** Player commands, bot inputs, connects, carry loads, admin commands, payment events and calendar time enter the sim as logged inputs; what the log lacks, the sim cannot depend on.

## Session log

- **A snapshot belongs to the release.** A snapshot is the postcard encoding of every state component and resource by stable id, with `StateRegistry::DATA_VERSION`, raised whenever the format changes, as Minecraft's `DataVersion`; a restore refuses another version before it reads a type, and a golden pins each type's layout with the version.
- **A restore writes each entity once.** A restore decodes every section first, then writes each entity, in stable-id order, with its stable id and every value of it in one move between archetypes, so a hook that runs as its parts come finds the entity whole. Refused: each section's values inserted as it decodes, which moved each entity once for each state type it holds.
- **A hook skips a restore's rules.** A hook that asserts a rule of the state, as one kind's marker on an entity, skips the rule while a restore puts in its untrusted values, which the restore's checks then refuse with an error, never a panic; Bevy has no switch that stops the hooks during a load. Refused: each such rule checked again on the decoded values before the write, a second form of each, and no assert in a hook, which loses the check of a second kind outside a restore.
- **A hash for each state type.** One BLAKE3 hash per state type, then one over the list, so a mismatch names its first divergence ([Determinism core](07-determinism-core.md)).
- **A registry's types are fixed as it builds.** The installs register the state types in a `StateRegistryBuilder`, whose `build` gives the `StateRegistry` a match holds, which registers nothing, so the hash, the snapshot and a record of a world's changes, which holds each type by its place, read one fixed list; a test that opens a built match takes its types back into a builder, which refuses a new type once a record holds them. Refused: one registry that registers, encodes and records, with a flag that refused a type after its world's changes were tracked, a rule each caller learned only at run time.
- **Each state type walks the entity index.** A type's hash and its snapshot section read each entity's value in stable-id order from the entity index, which gives the canonical order with no sort. Refused: a query of the type's holders, sorted by id, which made the hash and the snapshot 5 % to 6 % slower, as the encoding, not the walk, costs the time ([State hash walk](../research/state-hash-walk.md)).
- **No slow tick for a checkpoint.** At a boundary the server copies only what changed since the last checkpoint, and a background thread encodes, hashes and writes it ([Sessions](08-sessions.md)).

## Saves

- **A save is a kept checkpoint.** A save is the snapshot at a tick boundary and the log before it.
- **One-way converters.** Each release converts saves from the data version before it, one way, as Factorio and Minecraft's DataFixerUpper do; an older save runs them in order, and a converted save starts a new segment whose proof starts at the conversion.
- **Carry is an input.** The mode declares a typed, versioned carry schema; a session loads a carry as an external input at its start, so its log holds everything it read.

## Storage

- **`store` owns every file and thread.** One layer gives each read and write one place to keep its promises: a bound on what it reads, a refusal of a link, a durable write, and an error of cases for each failure. The crate that defines a file's bytes owns what they hold.
- **Durable writes.** A write goes to a temporary file, syncs it, renames it over the target and syncs the directory; each step's failure is its own error and is never retried ([Sessions](08-sessions.md)).
- **Secret files.** `SecretFile` is user-only, gives no `&Path` back, refuses a file others may open, never replaces a secret on create, and holds its bytes in `Zeroizing` buffers.
- **`protocol` does no IO.** The session log writes framed records into a `RecordSink`, which `net` implements over `store`.
- **One thread for each job.** Every thread starts through `store`'s `Worker`, named and joined; none waits for the disk or allocates in a steady state, and each keeps its failure for its holder, so a failure at shutdown reaches the exit code.
- **A data directory is locked.** `DataDir` exists only while it holds its directory's lock, so a second process on one directory is refused, and refuses a directory others may open, as Tor does.
- **Reads follow no link.** A read opens with `O_NOFOLLOW`, or the link itself on Windows, and refuses a path whose last name is a link, as OpenSSH opens a key; a caller that means a link's target, as the Zero Hour import reads a player's install, names the target first. Refused: reads that follow links, by which a link left in a data directory reads a file elsewhere.
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
- **User-only files.** Mode 0600 or 0700 on Unix; on Windows a protected DACL for the user, a file checked as OpenSSH for Windows checks a key.
- **Durable names.** A rename then a directory sync on Unix, `MoveFileExW` with write-through on Windows, never retried.
- **Stop signals.** The first `SIGINT`, `SIGTERM` or `SIGHUP`, or console event on Windows, raises a flag the app reads; a second ends the process with code 130.
- **Unsafe only where an outside API asks.** Win32 through Microsoft's `windows` crate, `libc` on Unix, Bevy's `SystemParam` for the network stack's synced timelines, `polling`'s registration of a socket with its poller, and `GlobalAlloc` for a test's counting allocator, each block saying why it is sound; the one other is math's wide division ([Determinism core](07-determinism-core.md)).

## Scripting

- **Rhai, not `bevy_mod_scripting`.** `script` alone names Rhai, and scripts call only the registry's API, never the ECS; `bevy_mod_scripting` exposes all of Bevy and pins its patch versions.
- **A raw engine.** `Engine::new_raw` with only the packages scripts need, strict variables, a fixed hashing seed; no `eval`, imports, floats or time.
- **Operation pools.** A limit per call, and per-tick pools the manifest sets, counted the same everywhere, so an over-budget script fails the same everywhere. A player's calls spend the player's pool, the `on_input` of each mode input the player sends among them, so one player's flood of inputs spends no other's pool, as Ethereum's sender pays for the work it causes; AI calls and the calls of units no player controls spend `think`, a modifier's from no source and the mode's other hooks `mode`; `calc_damage` and `calc_heal` have no pool, so a crowded fight cannot change how they weigh. A mode hook or an AI that finds its pool spent runs first in a later tick; a call a player causes fails as over budget. One rule picks the pool from the call's start, and the script batch answers each hook's end by that policy, so no caller chooses either.
- **All or nothing per call.** A call's state writes go to an overlay and its effects queue in its frame; on success both apply in call order, on failure both are dropped and a `script_error` is recorded.
- **Declared script state.** Script state is declared and stored in sim components, one `StateSchema` for each declaration, the mode's, a unit type's or a modifier's, so no script keeps hidden state ([Script state](03-game-scripting.md#script-state)).
- **A batch's calls share what no call changes.** Each call extension takes, as its batch starts, what every call of the batch reads alike, as the tick's draws and the book of params; a call takes only what an earlier call's effects of the batch may change, as the mode's state and the players' resources. Refused: each extension's whole read at each call, which made the check of the tick's draws 1.9 % of a 3v3 run's samples ([Server tick profile](../research/server-tick-profile.md)); and a copy at first use, as a call's functions reach no world.
- **A filter's text resolves once.** The view keeps each filter a script gives as text, found first by the identity of the shared text Rhai passes for a literal, then by its letters, up to a bound past which a text resolves at each call. Refused: a parse at each call, 1.5 % to 2.5 % of a 3v3 run's samples ([Server tick profile](../research/server-tick-profile.md)).
- **A scripted pass collects in a plain system.** A plain system collects what a scripted pass runs in the tick into the pass's `Due` scratch, in the order the pass runs it, and the pass's exclusive part only runs the scripts; a check or a step that an earlier call's effects may change stays in the exclusive part, between the calls, as a channel's step does. Refused: one exclusive system that collects and runs, which holds the world for the whole pass and hides what the pass reads.
- **A shared index refreshes in its own system.** The unit grid builds again, when its tracker finds the units changed, in a system that runs right before each reader, so its readers take it as `Res` and run side by side; a system that changes the units and reads the grid within its run refreshes it in between. Refused: a build by the first reader that asks, which made every reader hold `ResMut` and run alone; and one refresh a stage, which a change within the stage leaves stale. As a physics crate's broad phase updates in its own set before its queries.
- **The view refills what changed.** A read of the script view fills again only the rows whose sources changed, found by `Changed` filters derived from the parts' types, and the rows of units that lost a part a source reads or went, which observers of each read part's removal record, as no change tick shows a removal; it visits only those units, copies the rows between them in runs, and changes nothing when it touches none. A debug build reads twice and fails when the reads differ. Refused: a walk of every unit at each read, to find the units that came, went or changed their parts, which made a later read of a tick, with one unit in a hundred changed, cost two thirds of a full read ([Script view reads](../research/script-view.md)).
- **Each match compiles its scripts.** A match build takes 3.6 ms, once for each session and each verifier checkpoint and never in a tick ([Script call cost](../research/script-call-cost.md)). Refused: shared ASTs, an engine prototype, books for each package fingerprint and handles without `Rc<View>`, which save that once.
- **Hot formulas go to data, planned.** A formula a pass runs for each item, as `calc_damage` for each damage entry, moves to data a native stage computes, as each Rhai call boxes its numbers, once a bench measures it.

## Backends

- **One interface, pluggable backends.** Collision, pathfinding, movement and visibility each have one interface, and an imported game's own behaviour is a backend of it ([Navigation](04-capabilities/navigation.md)). A physics backend may use strictly deterministic floating point inside itself, as Rapier's `enhanced-determinism`; everything else is fixed-point.

## Bevy

- **The sim is one schedule on one thread.** `sim` uses `bevy_ecs` alone, on one thread: a tick's systems are too small to share, and the parallel executor made a 3v3 tick cost 1.26 times as much on the mean and 1.42 times on the worst ([Server tick profile](../research/server-tick-profile.md)). No pass gains threads inside itself: the move pass over Bevy's compute pool measured slower on both the crowded battle and the 3v3, as the pool's scope each tick cost as much as a 3v3 move stage ([Server tick profile](../research/server-tick-profile.md)).
- **A process runs one match.** A host runs a server process for each match, as a 3v3 tick takes 75 µs of its 33 ms on one core ([Cell levels](../research/cell-levels.md)). Refused: many matches in one process, one thread each, which needs one endpoint for every match, a runner that sleeps to the next deadline and a fault for each match, for a density no host needs.
- **Every schedule of the server runs on one thread.** `SimServerPlugins` sets the single-threaded executor on each schedule as the app finishes, as Cargo turns `multi_threaded` on wherever a build's member enables it, and Lightyear's unordered systems would otherwise run in an order set by timing.
- **The server's frame declares its order.** Its `Update` and `FixedUpdate` are strict schedules, as the sim's is: the frame's steps are chained sets after Lightyear's own, an app's own system goes after them, and two systems of conflicting access with no order between them fail the build. Refused: the order the single-threaded executor picks, which no code states and a new system changes; marking Lightyear's sets ambiguous with the frame, as the frame's exclusive systems do touch what they touch.
- **The server's life is a Bevy state.** `ServerPhase` names the phase each system of the frame runs in, and one call opens a session with its phase. A phase changes at the next transition, so a session that ends within a frame stops its systems by its log's result. Refused: the set of resources that exist as the phase, which no type lists; an `Ended` phase, a second record of the result the log holds; building the match in `OnEnter(Running)`, which would keep the lobby's log and links in a resource for a frame.
- **The frame checks the packets' signatures on its own thread.** A packet's record is almost all its chain head's check, about 19 µs, so 32 players' packets take under 2 % of one core at 30 ticks a second, and the worst frame, a burst of 9 ticks after a stall, about 5.6 ms ([Signature checks](../research/signature-checks.md)). Refused: checking the seats' packets on worker threads and applying them in seat order, which saves under 0.6 ms a tick at 32 players and pays a wake of each worker every frame.
- **A match is a world the release built.** `Release::build(world, spec)` is the one build; the `App` owns the world on the server and the client, as Lightyear replicates only its main world, and `Match` everywhere else. The match's operations are traits on `World`, so every host reads the same.
- **A world with no `App` clears its trackers.** A replay or a test's `Match` clears change trackers after each tick, as an `App`'s update does, so a long replay holds removals in bounded memory.
- **A tick's removals are the tick's.** A sim system reads a part's removals through `Removals`, which an observer fills and the tick ages, never Bevy's `RemovedComponents`, which an `App` ages at each of its updates: a server updates many times a tick and a replay once a tick, so a removal after its reader ran would reach the replay's reader alone. A test of `rules` refuses `RemovedComponents` in the sim's crates.
- **A client predicts what its player controls.** Lightyear keeps the client's tick ahead of the server's; a rollback reruns the sim from the server's state; it predicts movement, never a random outcome, and runs no effect and no script.
- **Floats only in the client.** `Transform` exists only in the client, which draws each unit interpolated between ticks.
- **Determinism rules.** Ordered stages, with a strict schedule that fails on an ambiguity or a redundant edge; stable entity ids, never Bevy's `Entity`, as Bevy does not guarantee query order; fixed-point positions within ±2²⁰ m; no floats, seeded hash maps or wall clock; a cryptographic PRF as the RNG, so no client can recover the seed ([Determinism core](07-determinism-core.md)).

## Client

- **A client module list.** The client's window end is one list of modules, each with the capabilities a mode holds for it and the plugin it adds, as the sim's modules are one list, so a client of any mode adds what its capabilities serve and no system takes a part of a capability the mode lacks as an `Option`. Refused: an `if` for each capability where the plugins are added, which a new panel forgets.
- **Package assets through a package source.** Models and textures reach Bevy through `package://<place>/<path>`, whose reader checks each file against its package's index, and Bevy's own glTF and KTX2 loaders parse them on demand.
- **At most four glTF loads at a time.** Bevy's glTF loader waits on a task scope, and its thread runs the next queued load inside that wait (Bevy #21954), so the package source takes one of four permits before it opens a `.glb`, and a load that waits yields instead of nesting.
- **Material extras in a glTF extension.** What a glTF material cannot say, an additive or multiplying blend, a blend with an alpha test, no depth write, a depth bias for a layer, is the vendor extension `CAMPFIRE_material_extras` on the material, as `KHR_` extensions add to glTF, and the rest is glTF's own, its texture with the sampler glTF gives each texture; the client's glTF extension handler gives such a material its extension of Bevy's standard material as the model loads, once for each model, as Bevy's own material handler does. It is no lighting rule, as every material lights alike. Refused: a material file beside the model that restates the whole material, and its texture's sampler set on the image every material shares.
- **Team colors in the one material.** Every model's material is Bevy's standard material extended by its extras, and a mesh's tag names two places in one buffer of the match's colors: its unit's team's, which a material its extras paint takes as a tint of its color, or as its hue where its texture's alpha marks a mask, as Zero Hour paints a mesh it names `HOUSECOLOR` and a texture of `ZHCA`; and a tint that adds to its light, as Zero Hour tints a build that may not stand. The mode's `client/team_colors.toml` gives each team's color, which the Zero Hour import takes from the game's `MultiplayerColor`s. A unit of no team draws a tinted material white. Refused: a material or a recolored texture for each team, as the game makes them, which multiplies the materials by the teams; and the color itself in the tag, which leaves no room for a tint.
- **A look hides its own nodes.** The nodes a look hides are its own, as Zero Hour's draw modules hide a model's sub-objects by their state, and a few models' looks hide different ones, so each instance hides them as its scene spawns. Refused: hidden nodes at the model's load, which would need a model for each look.
- **The import measures a look's height.** The import writes each model's top in the client file: the highest corner of each primitive's accessor bounds, placed by its nodes, with the look's hidden nodes hidden, as Bevy's glTF loader bounds each primitive. A drawing takes its height from it as it spawns, and the client keeps its meshes on the GPU alone. The import measures it by its own fixed-order `f32` steps, as it places a model's vertices, so every machine writes the same bytes. Refused: a pass that walks each drawing's spawned tree every frame until its bounds come, which also keeps a copy of each mesh in RAM; and glam's types, whose SIMD paths only attempt the same bits on every platform (glam's README, `fast-math`).
- **A terrain is quads over the height grid.** `client/maps/<name>/terrain.bin` gives each cell a ground quad with its own texture coordinates and diagonal, and overlays over them in drawing order, so it holds an old tile terrain, Zero Hour's tiles, blends and stretched cliffs, as its renderer draws them, and a splat terrain as overlays; it names no rule of a game. Coordinates are `f32`, as precision comes before the disk: sixteenths of a texel would halve the size and round a cliff's stretched coordinates.
- **The terrain's texture is composed at load** from copies of package textures and border wraps, so the package holds no second copy of its texels.
- **The terrain is lit as every model is,** by the map's lights and normals from the height grid; no game's lighting rule enters the client.
- **No glTF or splat for the ground.** A glTF of each map's ground costs four times the terrain file; a splat terrain of tiled layers, as Unity's and Unreal's are, cannot hold a tile terrain, as each cell draws its own tile.
- **The drawn ground is the terrain's.** A unit, a client object and the pointer's ray stand on the triangles the terrain draws, which may differ from the sim's ground, as the original's logic and renderer differ ([Space and map](04-capabilities/00-overview.md#space-and-map)).
- **The client draws a projectile with its look.** A projectile of a look draws its models at its point, its height included, turned along its last step, and one of no look a ball, so a game's shell needs no drawing of its own; a turreted unit's look names the node its turret angle turns.
- **A selection of the player's own units.** A left click selects one, a drag selects those in the rectangle once the cursor moves past 4 pixels, Windows' default; a right click orders the selection as one group, as StarCraft II's does.
- **Bars in screen space.** A unit's bars stand over its drawing's top at a fixed size in pixels, as Zero Hour's `Drawable::drawHealthBar` and StarCraft II draw theirs on the screen, so a match of any scale reads alike; each stand turns to face the camera and scales to a pixel at its depth, in the one pipeline. The mode's `client/hud.toml` says which units show them: all, or the selected and the pointed only, as Zero Hour shows them. Refused: bars sized in meters, which a map of another scale shows too small or too large.
- **A click picks by the drawn volume.** The ray through the cursor picks the unit whose drawing it meets first, its footing raised by its height, as Zero Hour's `pickDrawable` casts the ray against what it draws; a pick by the ground point under the cursor misses the upper part of a tall building. A triangle-exact pick, as Bevy's mesh picking does, would keep every mesh in the main world.
- **A command panel from the actions.** The panel shows the first selected unit's train and build actions and its queue, from the sim's own data, so any game's production plays with no panel of its own.
- **The input's modes are states.** Whether a click selects and orders or places a build is a Bevy state, which the selection, the orders and the build preview each run in, as Bevy's own `States` drive a schedule; the build placed is the state's resource, which goes as the state leaves. Refused: a mode held by whether a resource exists, which each reader tests apart.
- **Text by Bevy's own UI.** `bevy_ui` and `bevy_text` with the embedded font add no crate outside Bevy, where `bevy_egui` would be a new dependency and a second UI system.
- **Every key on a keyboard with no keypad.** The camera turns and zooms by Alt with the arrows beside Zero Hour's keypad keys, and Space pauses a local match as in Paradox's games; the middle button pans, keeping the grabbed ground under the cursor, as Unity, Unreal and StarCraft II do, and orbits with Ctrl, as Blender's turntable.
- **Keys are the mode's data.** The client's keys are the mode's `client/keys.toml`: each action a list of chords, a key by its W3C `KeyboardEvent.code` name with the modifiers held, matched exactly as Zero Hour's `MetaMap` matches its modifiers, and an action the file does not give keeps the engine's own keys. The camera file gives the width of the edge that scrolls and the time of a click, which the Zero Hour import takes from the game's `edgeScrollSize` and its `Mouse.ini`'s `DragToleranceMS`. Refused: Bevy's `KeyCode` names, which change with Bevy's releases as a package's format must not; and an input crate, as the client needs no more than chords.

## Creator tools

- **Data schemas.** A JSON Schema for every data file, generated from the engine's types, planned, so an editor completes and checks a package's TOML.
- **Hot reload is unverifiable.** A dev session reloads scripts and data without a restart, as Roblox Studio does; its terms say so, and the verifier refuses its log.

## Testing and diagnostics

- **Scenarios on a modeled link.** Whole matches between scripted players run through a link of delay, jitter and loss on a manual clock, so each run repeats; a match played with its links reversed proves the server orders joins and inputs by key and seat.
- **Two goldens.** The state golden pins each tick's state hash and moves with the layout; the behaviour golden pins a digest of each tick's units, deaths and resources, and does not.
- **One scripted match for each mode.** A new rule of a mode joins its scripted match, the one test exempt from the 1 s bound; the suite keeps 30 s.
- **nextest.** A process per test, as libtest cannot end a stuck test; a test is slow past 1 s, and no profile stops at the first failure.
- **Allocation audits.** A crate's `tests/alloc` counts each hot path's allocations a frame at a time, zero unless a fixture names each one it allows. A window counts every thread of the process but its main one, as nextest runs each test in a process of its own, so a fixture's workers count too; a thread's excluded part is its own. The main thread is libtest's harness, which runs the test on a thread of its own and allocates as it first waits for it, so the allocator marks the thread of the process's first allocation and counts none of its allocations. `campfire-measure` holds the one allocator and audit of the workspace. Refused: a window of the test's thread alone, which passes a worker that allocates; a counter of each crate's own, as three were.
- **One test feature.** What only tests and benches build is behind `internals` and one gate; `#[cfg(test)]` holds a crate's own test modules alone, and every test build turns `internals` on.
- **The archetype shuffle.** A test moves every unit to a new archetype before each tick and checks both goldens, so no order depends on archetypes.
- **The work record.** A redesign records the instruction count of the proving match and the 3v3; a stage that makes either worse by more than 10 % says why.
- **The real binaries on localhost.** The LAN check runs the shipped server and clients over QUIC and checks their logs with the verifier.
- **Warnings fail tests.** A fixture that runs a match holds a `LogCheck`, and each warning no test takes fails it, as rustc's tests and pytest's `-W error` do. The check is the process's subscriber, so a warning of a worker thread counts as the test's own.
- **Typed log events.** An event a tool reads back is a `LogEvent` with a round-trip test; `sim` and `capabilities` log nothing, and report through resources.

## Benches

- **One bench target for each crate,** whose cases share one id scheme, `<kind>/<group>/<case>`, chosen by criterion's filter.
- **The gate runs each case once.** The gate runs the bench lane with Criterion's `--test`, which runs each case once, so a bench that no longer builds or runs fails it, on the build `cargo drive bench` shares. Refused: the benches outside the gate, where two breaks of the gather bench went unseen, and a run of the bench targets as tests, a new cache shape built without the bench profile's optimizations.
- **One selection for every run.** The bench lane builds exactly the crates with a `[[bench]]`, each with `internals`, so a case builds with the same features whichever cases run.
- **One sampling policy.** A group takes its kind from `campfire-measure`'s `Kind` and its sampling from its `Scale`, a case of microseconds or a frame of milliseconds, the one place a group's sampling is set. Refused: a kind spelled in each group's name and a sample size set by hand, which gave each group a sampling of its own and the workspace two schemes of ids.
- **Atomic and integration cases.** An atomic case runs one operation over 4,096 inputs; an integration case runs a system for a tick on a scene of 1,000 units, the scale of a large Zero Hour battle, or one end on the test packages.
- **The worst tick is the budget.** An end has a `mean` and a `worst` case, as the worst tick sets the budget ([Tick rate](04-capabilities/00-overview.md#tick-rate)).
- **Stage probes keep the hashes.** `StageClock` times each stage of the real tick from outside the state.
- **Frame pointers for profiling.** Profiling builds with frame pointers in its own target directory, as DWARF unwinding loses the optimized stacks and AMD has no LBR.
- **Code placement moves a case.** The same machine code at another cache-line offset runs at another speed, so a change of a few percent is read against the machine code, not the time alone ([Math performance](../research/math-performance.md)).

## The driver

- **One owner of every cargo invocation.** `cargo drive`, the `drive` submodule at `source/drive`, the same driver in every project, runs every check, test and bench, as each shape of a cargo command keeps a cache of its own and a `-p` shape rebuilds tens of units for no speed. What the workspace adds to it is data, in `source/Cargo.toml`'s `[workspace.metadata.drive]`. Refused: a script of the project's own, `drive.py`, whose rules drifted from those of the other projects' drivers.
- **A lane is one cache shape.** Each lane covers the workspace and runs `--locked`; no command takes a crate list.
- **The ship lint leaves out what unifies `internals`.** A member whose normal dependency turns `internals` on is left out of the shipped build's lint, which `lint` covers.
- **A check is the format and the lint.** `check` runs `fmt` and the lint of every target with every feature, and ends at its first failure, in seconds; `test` writes each package's index again, then runs the tests; `gate` runs every lane. Refused: the tests in each check, which made a check of a one-line change wait for every test binary.
- **A step runs again only for what a change reaches.** A step that passed keeps the tree of files it passed on, and runs again only when a file it reads changed; a test step runs only the crates a change reaches and their dependents. A member whose tests read files outside its directory declares them, as the workspace's rules read every crate and the design documents, so a change of a document reaches them. A command's steps share one pass, as a later step runs what an earlier one built, which the tests' build writes over.
- **Measured, not assumed.** Each command appends its steps' times and results to the driver's run log, a file a month in the OS's state directory.
- **A local run stops at its first failure, and CI runs on.** A local `check` or `gate` formats the code and ends at its first failure, as its fix comes first; `gate --ci` checks the format and runs every step, so one run, of half an hour, lists every failure, and the driver refuses a local `gate` in CI, which would format the checkout and stop early. Refused: CI that stops at its first failure, which a second failure takes another run to show.
- **No docs lane.** The workspace's drive table leaves rustdoc and the doc tests off: no crate's docs are a product, and each doc test is a binary of its own.
- **A release keeps its debug info beside its binaries.** `cargo drive package` builds the release profile of the shipped binaries, its debug info full, and moves it out of each binary into a file beside it: `objcopy --only-keep-debug`, then `--strip-debug` with a GNU debug link, on Linux; a packed `.dSYM` and `strip -S` on macOS; the linker's own PDB on Windows. Refused: Cargo's packed split on Linux, which keeps skeleton DWARF in the binary, so a strip of it orphans the `.dwp`.

## Networking

- **The protocol is apart from the transport.** `protocol` is the versioned, documented format a replay needs; `net` uses Lightyear over `bevy_replicon` for transport, replication, prediction and interest, and links over QUIC alone ([QUIC transport](11-quic-transport.md)).
- **One release on both ends.** Client and server always run the same engine release.

## Libraries

Exact versions are pinned; each release tag pins its toolchain and `Cargo.lock`, so a build can be compared with the published binaries. Release trust follows [TUF](https://theupdateframework.io/docs/security/).

| Area | Crate | Why |
| --- | --- | --- |
| Fixed-point numbers, trig, roots, SIMD lanes | Own code in `math` | Exact and portable ([Determinism core](07-determinism-core.md)); `std::simd` is unstable and intrinsics need `unsafe` |
| RNG | `blake3` keyed hash | A PRF by specification, counter-based as Random123; `rand` may change output in minor releases |
| Binary encoding | `postcard` | A stable wire format since 1.0 |
| Data files, log lines | `toml`, `serde_json` | Through `codec` |
| Client files' vectors | `bevy_math` | The client draws them with no conversion; the sim's vectors are `math`'s |
| State hashes | `blake3` | |
| File and package fingerprints | `sha2` | SHA-256, as Blossom addresses blobs |
| Secret buffers | `zeroize` | Already in the build through `rustls` |
| QUIC | `quinn-proto`, `quinn-udp`, `polling` | QUIC with no IO and no async runtime ([QUIC transport](11-quic-transport.md)) |
| Stop signals | `signal-hook` | A handler that does only what POSIX allows |
| Port mapping | `igd-next`, `netdev`, `socket2` | UPnP-IGD by its blocking API, and the default gateway for PCP and NAT-PMP |
| TLS | `rustls` over `ring` | TLS 1.3 alone, a raw pinned Ed25519 key |
| Errors, `Display` | `thiserror`, `derive_more` | |
| Command lines | `clap` | |
| Nostr | `nostr`, `nostr-sdk`, `nostr-connect` | Still alpha |
| Lightning | `nwc` | Drives the host's own wallet, hold invoices included; no embedded node |
