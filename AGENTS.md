# Campfire

An open-source (MIT/Apache-2.0) Rust engine for multiplayer games, plus importers that bring existing games onto it from the player's own copy, Generals: Zero Hour first. The sim is deterministic and server-authoritative: every session log can be replayed and verified. Game content is Rhai scripts and data in signed packages. Identity, server discovery and the marketplace use Nostr. Lightning payments are an optional module.

## Layout

- `docs/` — the project's documents:
    - `design/` — the design, and the source of truth for every decision. Read it before you change anything:
        - `01-campfire-design.md` — vision, terms, principles, milestones
        - `02-engine-core.md` — modules, structural rules, storage, determinism rules, Bevy and Lightyear, testing and benches, libraries
        - `03-game-scripting.md` — packages, tick pipeline, script state, numbers, network sync
        - `04-capabilities/` — the mechanisms the engine provides (combat, commands, character, navigation, vision, …), which a game combines; `games.md` lists the imported games
        - `05-protocol-spec.md` — keys, connection, session log, verification, Nostr events, payments
        - `06-research-notes.md` — early decisions, risks and sources
        - `07-moba.md` — the MOBA test content: heroes, player spells, rules
        - `08-script-api.md` — script API by capability, derived from the packages the engine plays
        - `08-script-api-reference.md` — every name of the script API, with its roles, capability and whether it runs; generated from the registry by a test, never edited by hand
        - `09-determinism-core.md` — the determinism core: numbers, vectors, randomness, stable ids, state hash, each operation one `i128` step rounded once
        - `10-sessions.md` — sessions: journal, crash restore, slots, reconnect, server bots, receipts, checkpoints, saves, local server
        - `11-rts-foundations.md` — box bodies, package unit types, group orders, production's first cut
        - `12-zero-hour.md` — the first imported game: importer, rules package, parity oracle, presentation
        - `13-bodies-uses-shop-gravity.md` — bodies up to 2,048 m and the spatial indexes' levels, the `use` action kind, the client's shop and item keys, projectiles that fall
        - `14-replication-audiences.md` — who receives each part of the state: the kind on each state type, the audiences a mode and its host set, spectators and players out of play, the player, team and match entities
        - `15-quic-transport.md` — the link over QUIC alone, on `quinn-proto` under Lightyear
        - `16-terrain.md` — the terrain: one generic format of quads over the height grid, drawn by the one rendering pipeline, which an importer converts its game's terrain to
        - `17-units-that-drive.md` — the first units that move: a skirmish's start, the turn rate, the client's selection, cliffs as walls, Zero Hour's movement on the generic backends
    - `research/` — what a system's design is taken from: measurements and the sources read, facts only; `zero-hour.md` for the import, `quic.md` for the transport, `spectating.md` for what spectators and players out of play see, `math-performance.md` for the measures of the math and geometry benches
- `.notes/ISSUES.md` — the issue log: every open problem, under the step it waits for ([Issue log](#issue-log))
- `source/` — engine and game code, and `drive.py`, the driver of every check ([Checks](#checks)); `source/packages/test/` holds the packages the tests play, the MOBA test content (heroes, spells, modes) in `test/moba/` among them; `source/packages/<game>/` holds an imported game's rules package
- `../GeneralsX/` — beside this repository, not in it: a checkout of [GeneralsX](https://github.com/fbraz3/GeneralsX), the released Generals and Zero Hour source with TheSuperHackers' fixes, built for Linux and macOS. It is the reference for Zero Hour's formats and rules, and the base of the parity oracle. It is GPL: read it as a specification, and copy no line of it ([Zero Hour](docs/design/12-zero-hour.md#decisions), D2).

`PLAN.md` is the one plan: every open step of the project, in the order it is done, in batches of one commit each, and the foundations ([Workflow](#workflow), [Issue log](#issue-log)). It lists open items only.

When code and design disagree, fix one of them, and state which one.

## Direction

Every change moves the engine toward one shape. The rules and the tests that hold them are design 02's [Structural rules](docs/design/02-engine-core.md#structural-rules).

- **What can be forgotten is a type or a trait method with no default.** A rule that no type can hold is held by a type that makes the right way the short one, a test where one can see the breach, and review.
- **A module is one entry of one list.** Each module, the core's, each capability and the mode, implements `Module`: its capability, its needs and its uses, `declare` and `install`, none with a default. `install` holds only its systems, each in one of its module's sets; all else a module adds is a declaration.
- **What a module adds is declared once,** and one walk of the declarations serves the install, the script API's binding, the network's lists of state types and the declaration audit. So a new capability is one entry of the list, a new state type its type and one declaration, a new effect an effect type and its callback; the audit and the release tests fail on each piece left out.
- **A kind is a marker,** one on each entity, and an entity spawns whole: each part it carries is declared on its kind and comes with its spawn, so a new entity moves between archetypes once. A part derived from another comes by Bevy's required components or that part's hook; a part a system derives each tick is one it writes in place. A marker of a state an event starts and ends, as `Dead`, comes and goes by command.
- **A name is resolved once,** at the load: one table of names for each kind of id, a builder that is the check of what it resolves, a script argument resolved by its type.
- **One word, one meaning:** module, owner, kind, part, callback (`Hook` is the scripts' alone), `*File`, `*Data`, `*Spec`, `*Book`.
- **A pass visits what changed:** no query state, structural change or allocation per item on a hot path, and a fact fixed at install or at the load is a fact of the schedule, not a check each tick.
- **Each crate depends only on lower layers:** no two crates of one layer depend on each other, and no crate's top modules import each other in a cycle. What is the sim's lives in `sim`, and `math` holds the numbers every crate shares. A capability asks another for its work through a contract below both, never by writing its state.
- **One rendering pipeline for every game.** The client draws every game by one pipeline: one material and lighting model, Bevy's physically based standard material, and the generic formats it reads, glTF and the client files. An importer converts its game's materials, lights and formats to them, as near the original as it can, and writes down what the conversion loses. Where a game needs what the pipeline cannot say, the pipeline gains a general feature that any game may use, as the material file's blends and depth bias, never a path, a flag or a shader for one game. Only the importer knows its game.
- **Bevy's own mechanisms, not hand-made ones:** run conditions and `If` parameters, observers, one-shot systems, required components, sparse-set storage, plugin groups.

## Workflow

Work goes in vertical slices. A slice ends with something new that the user plays or sees in the client: a hero that walks its lane, a tower that shoots, a match that ends with a winner. A slice crosses every system it needs, and takes each one only as deep as the slice needs. It takes its items from the issue log and the plan. A slice goes in four steps, each agreed before the next starts:

1. **Decide** the slice: what the player does or sees at its end, in one or two sentences. The issue log lists the problems the systems it touches already have.
2. **Investigate** the code it touches, and how established engines and games solve the problem and why.
3. **Propose** a design of what the slice adds, written into `docs/design/`, with an implementation plan in `PLAN.md`, for review. The plan names every system the slice changes.
4. **Implement** the plan as reviewed, step by step, each step ending with `./drive.py check` passing. The slice ends when the user plays it in the client, and a scripted match keeps it working.

No changes to systems outside the plan along the way. A problem found outside the slice, or depth that the slice does not need, goes to the issue log, and a later slice takes it.

The plan, `PLAN.md`, holds the open steps and the foundations ([Issue log](#issue-log)): a step goes when it is done.

- **Batches.** A batch is one commit: the steps that change the same systems, refactorings, fixes and features alike. It says what it touches and why it stands where it does, and ends with `./drive.py check` passing and a stop for review.
- **Steps.** A step names what the code lacks against its design, or the design it proposes, which moves into `docs/design/` when it is agreed, and the part of the [Direction](#direction) it builds. A problem of the issue log that a step takes carries the step's tag there.
- **References.** A step's line numbers point to the code as the step was written, and the step refreshes them as it starts.
- **Checks.** A step that states no check holds this one: no golden moves, and `./drive.py check` passes.

## Issue log

The issue log is `.notes/ISSUES.md`, one file for every open problem of the project, by the rules below.

- **System.** Each item names, in bold, the system whose code or data its fix changes: an engine module of [Modules](docs/design/02-engine-core.md#modules), the core or a capability of `docs/design/04-capabilities/`, the MOBA test content, or an imported game. When that system is not known, it names the system where the problem shows, and the item goes under Research.
- **Sections.** The file has three sections, in this order, and an item sits in the section of its next step:
    - **Decide**: the options are known, and the user chooses one. The item names the question and its options.
    - **Research**: the cause, or the right fix, is not known.
    - **Ready**: the fix is known, and waits for its turn.
- **Items.** One bullet for each problem: its system, then what is wrong, where the code and the design differ, and not how to fix it. An item that a plan step takes carries its tag after its system, `**Net.** **Plan: F3.**`; an item with no tag waits for a slice to take it. Within a section, items go in the order of their systems' names.
- **Life.** An item moves between sections as it is triaged. A fixed item is deleted, with no done marker and no history. A section with no items stays, empty.
- **Foundations.** A problem whose cause is in the architecture or a chosen format, not in one piece of code, as several items show one cause or it limits every system built after it, also has an entry in the Foundations section of `PLAN.md`: its cause, what it limits, and the issue-log items it holds, named by system and the start of their text. Its own item in the issue log states the cause. The entry goes when its cause is fixed, and an item it names goes from it when that item is deleted.

## Checks

Every check, test, doc build and bench runs through `source/drive.py` ([The driver](docs/design/02-engine-core.md#the-driver)), never through `cargo clippy`, `cargo test`, `cargo nextest`, `cargo doc` or `cargo bench` by hand: each shape of a cargo command keeps a cache of its own, and the driver's shapes are the ones its commands keep built. No command takes `-p` or a crate list, as each covers the workspace, and none runs with its own `CARGO_TARGET_DIR`, `RUSTFLAGS` or `CARGO_PROFILE_*` but the profiling build of [Benches](docs/design/02-engine-core.md#benches).

| Command | Runs |
| --- | --- |
| `./drive.py check` | `cargo fmt`, the lint of every target, the lint of the build that ships, each package's index written again, and the tests of the crates the change since the last pass reaches: a step's check. A step does not run again when no file the workspace reads changed since it passed, and no step builds after a lint finds code that does not compile |
| `./drive.py index` | each workspace package's `package.index` written again from its files, after a change to a package |
| `./drive.py test [--bless] [nextest arguments]` | the suite, or a part of it: `-E 'package(campfire-net)'`, a test's name; `--bless` writes the pinned files again. It runs each time |
| `./drive.py doc` | rustdoc with `-D warnings` |
| `./drive.py bench [cargo bench arguments]` | the benches, only when the task asks for them |
| `./drive.py lan`, `./drive.py zero-hour [out]` | the LAN check, and the Zero Hour check, on request |
| `./drive.py gate` | every step CI runs, and the Windows lint on Linux |

Each step prints one line with its result; a failed step names its log, `target/drive/log/<step>.log`, which holds its whole output. Read the log, rather than run cargo again or pipe the driver through `grep` or `head`: what it prints is already filtered.

## Code

- **Banned calls go through their proxy crate**: files and threads through `store`, the formats through `codec`, a schedule's build settings through `schedule` ([Structural rules](docs/design/02-engine-core.md#structural-rules)). Only a proxy's crate root `expect`s `clippy::disallowed_methods` or `clippy::disallowed_types`; nothing else exempts a banned call, but a `thread_local!` that an outside API forces, whose module `expect`s `clippy::disallowed_types` at its head and names no other banned type. When a proxy lacks what a caller needs, the proxy gains it.
- **Each operation has one home, by what it acts on:** the whole match a trait on `World`, the match's content an associated function of its module's type, a value made from the world a constructor of its own type, a test fixture a struct of the ids it places ([Structural rules](docs/design/02-engine-core.md#structural-rules)).
- **Native only.** Campfire runs natively on Linux, macOS and Windows: no browser and no WebAssembly. Code, features, dependencies and design choices that exist only for a browser or a WebAssembly target are dropped, and the simpler native form takes their place.
- **No data in strings.** A value from a fixed set is an enum, a value with rules is a checked newtype, and an error is an enum of cases. Text from data files, scripts, JSON or the network becomes these types where it enters. Strings stay only for human text and for names the outside format defines.
- **All file I/O goes through `store`**, as [Storage](docs/design/02-engine-core.md#storage) says: every read, listing, check and write of a file, in production code, tests and checks alike. Production code uses its types; a test uses its internals for every file it makes or reads. No `std::fs`, no `Path::exists` and its kin, no `tempfile`: when `store` lacks what a caller needs, `store` gains it. A read names its bound, and no code asks whether a path exists: it reads or lists, and takes `Missing`.
- **Game-bound code lives in a module named after its game**, `zero_hour`, in the crate whose interface it serves, so no reader takes it for a general system; nothing but the place that installs it depends on it ([Modules](docs/design/02-engine-core.md#modules)).
- **Tests run on nextest**, never `cargo test`, which can end no stuck test. A new rule of a mode joins the mode's scripted match, not a new run; that match, in a module named `scripted_match`, is the one test exempt from the bound of 1 s ([Testing and diagnostics](docs/design/02-engine-core.md#testing-and-diagnostics)).
- **Benches run only when the user's task asks for them.** Optimization has its own sessions, which the user starts; until then a step's checks are its tests, and a bench case a step adds is built by clippy, not run. A case's id is `<kind>/<group>/<case>`, and its code a `bench.rs` beside what it measures, behind `internals` ([Benches](docs/design/02-engine-core.md#benches)): one case runs with `./drive.py bench -- --exact <id>`, the ids list with `-- --list`.
