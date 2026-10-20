# Campfire

An open-source (MIT/Apache-2.0) Rust engine for multiplayer games, plus importers that bring existing games onto it from the player's own copy, Generals: Zero Hour first. The sim is deterministic and server-authoritative: every session log can be replayed and verified. Game content is Rhai scripts and data in signed packages. Identity, server discovery and the marketplace use Nostr. Lightning payments are an optional module.

## Layout

- `docs/design/` — the design: the decisions every change follows ([Documents](#documents)). Read the documents of the systems a change touches before you change them. `06-script-api-reference.md` is generated from the registry by a test, never edited by hand.
- `docs/research/` — what the design is taken from: measurements and sources read, facts only.
- `PLAN.md` — the one plan: every open step, in the order it is done, in batches of one commit each, and the foundations ([Workflow](#workflow)). It lists open items only.
- `.notes/ISSUES.md` — the issue log: every open problem ([Issue log](#issue-log)).
- `source/` — engine and game code ([Checks](#checks)); `source/packages/test/` holds the packages the tests play, by game: `engine/` the generic heroes, modes and rules, `moba/` the MOBA test content; `source/packages/<game>/` an imported game's rules package.
- `../GeneralsX/` — beside this repository, not in it: [GeneralsX](https://github.com/fbraz3/GeneralsX), the released Generals and Zero Hour source with TheSuperHackers' fixes, built for Linux and macOS: the reference for Zero Hour's formats and rules. It is GPL: read it as a specification, and copy no line of it ([Zero Hour](docs/design/09-zero-hour.md#decisions)).

When code and design disagree, fix one of them, and state which one.

## Documents

A document holds what guides development and does not drift from the code, so keeping it true costs nothing when the code changes.

- **A decision, not a description.** A design document holds decisions: each is a bullet led by a short bold title, then the choice, the options refused and why, with its source, as an established engine, a game's own code or a measurement. One or two sentences; the code shows how.
- **Nothing the code, data or tests already state.** No field lists, formulas, counts, file layouts, check lists, which crate holds what, or what a system does step by step: the code, its doc comments and its tests are their record. A specification that third parties implement, as the protocol, is the one exception, for its byte layouts.
- **Facts go to `docs/research/`.** What a source says and what a measurement found, with its date, are research; the design cites them.
- **A slice's proposal is temporary.** A slice proposes its design in a document of its own, whose `Checks` guide its steps; when the slice ends, each lasting decision moves into the document of the system it governs, as its title and why, and the slice's document is deleted.
- **Code cites a decision by document and title,** `(04-capabilities/combat.md, Armor by damage kind)`, several apart by `; `, a path relative to `docs/design/`; a title holds no semicolon. A test of `rules` fails on a citation whose document or title is gone, and on a design cited by number.
- **Planned work is a decision too,** written where its system's decisions are, and marked planned.

## Direction

Every change moves the engine toward one shape. The rules and the tests that hold them are in [Structural rules](docs/design/02-engine-core.md#structural-rules).

- **What can be forgotten is a type or a trait method with no default.** A rule no type can hold is held by a type that makes the right way the short one, a test where one can see the breach, and review.
- **A module is one entry of one list.** Each module implements `Module`: its capability, its needs and uses, `declare` and `install`, none with a default; `install` holds only its systems, each in one of its module's sets, and all else is a declaration.
- **What a module adds is declared once,** and one walk of the declarations serves the install, the script API's binding, the network's lists and the declaration audit, so a new piece left out fails a test.
- **A kind is a marker, and an entity spawns whole:** each part it carries is declared on its kind and comes with its spawn, so a new entity moves between archetypes once; a derived part comes by required components or a hook, and one derived each tick is written in place.
- **A name is resolved once,** at the load, by a builder that is the check of what it resolves.
- **One word, one meaning:** module, owner, seat, slot, kind, part, place, callback (`Hook` is the scripts' alone), extension, `*File`, `*Data`, `*Spec`, `*Book`, `*Decl`, `*Setup`, each as [Names](docs/design/02-engine-core.md#names) gives it.
- **A pass visits what changed:** no query state, structural change or allocation per item on a hot path, and a fact fixed at install or load is a fact of the schedule, not a check each tick.
- **Each crate depends only on lower layers,** and a capability asks another for its work through a contract below both, never by writing its state.
- **One rendering pipeline for every game.** An importer converts its game to Bevy's standard material and the generic formats, and writes down what it loses; a need the pipeline cannot say becomes a general feature, never a path, a flag or a shader for one game.
- **A capability serves every game.** It names no game, genre or constant of one game. What a game expects is configuration: its rules package supplies it as data and Rhai, and the import ships that package with the game. A need that no configuration can express becomes a general option of the capability, never a path or a flag for one game ([Games](docs/design/04-capabilities/games.md)).
- **Bevy's own mechanisms, not hand-made ones:** run conditions and `If`, observers, one-shot systems, required components, sparse-set storage, plugin groups.

## Workflow

Work goes in vertical slices. A slice ends with something new that the user plays or sees in the client, crosses every system it needs, takes each only as deep as it needs, and takes its items from the issue log and the plan. A slice goes in four steps, each agreed before the next starts:

1. **Decide** the slice: what the player does or sees at its end, in one or two sentences.
2. **Investigate** the code it touches, and how established engines and games solve the problem and why.
3. **Propose** its design in a document of its own under `docs/design/` ([Documents](#documents)), and its plan in `PLAN.md`, naming every system it changes, for review.
4. **Implement** the plan as reviewed, step by step, each step ending with `cargo drive check` and `cargo drive test` passing. The slice ends when the user plays it in the client and a scripted match keeps it working; its decisions then fold into the system documents.

No changes to systems outside the plan along the way: a problem outside the slice, or depth it does not need, goes to the issue log.

- **Batches.** A batch is one commit: the steps that change the same systems. It says what it touches and why it stands where it does, and ends with `cargo drive gate` passing and a stop for review.
- **Steps.** A step names what the code lacks against its design, and the part of the [Direction](#direction) it builds; an issue it takes carries the step's tag. A step's line numbers point to the code as it was written, and the step refreshes them as it starts. A step that states no check holds this one: no golden moves, and `cargo drive check` and `cargo drive test` pass.
- **A step goes when it is done.**

## Issue log

The issue log is `.notes/ISSUES.md`, every open problem of the project.

- **System.** Each item names, in bold, the system whose code or data its fix changes, or, when that is not known, the system where the problem shows, under Research.
- **Sections,** in this order, each item in the one of its next step: **Decide**, the options are known and the user chooses, the item naming the question and its options; **Research**, the cause or the right fix is not known; **Ready**, the fix is known and waits for its turn.
- **Items.** One bullet each: its system, then what is wrong, not how to fix it. An item a plan step takes carries its tag after its system, `**Net.** **Plan: F3.**`. Within a section, items go by their systems' names.
- **Life.** A fixed item is deleted, with no history; a section with no items stays.
- **Foundations.** A problem whose cause is in the architecture or a format, as several items show one cause, also has an entry in `PLAN.md`'s Foundations: its cause, what it limits and the items it holds. The entry goes when its cause is fixed.

## Checks

Every check, test and bench runs through `cargo drive`, from `source/` ([The driver](docs/design/02-engine-core.md#the-driver)): `cargo-drive`, the same driver in every project, whose lanes and the workspace's own commands are in `source/Cargo.toml`'s `[workspace.metadata.drive]`. It is installed with `cargo install --locked --git https://github.com/xorza/drive`; the table's `version` names the driver the workspace needs, and an older or newer one refuses to run. Never `cargo clippy`, `cargo test`, `cargo nextest`, `cargo doc` or `cargo bench` by hand: each shape of a cargo command keeps a cache of its own. No command takes `-p` or a crate list, and none runs with its own `CARGO_TARGET_DIR`, `RUSTFLAGS` or `CARGO_PROFILE_*` but the profiling build of [Benches](docs/design/02-engine-core.md#benches).

| Command | Runs |
| --- | --- |
| `cargo drive check` | `cargo fmt`, then the lint of every target with every feature, to the first failure: after each change |
| `cargo drive test [nextest arguments]` | each workspace package's `package.index` written again, then the tests, each time; with arguments, those nextest selects |
| `cargo drive bench [criterion arguments]` | the benches, only when the task asks for them |
| `cargo drive run lan`, `cargo drive run zero-hour [out]` | the LAN check and the Zero Hour check, on request |
| `cargo drive run vocabulary [out]` | the repository's layout and the names its Rust files declare, in `target/vocabulary.txt` unless `out` names another file, on request |
| `cargo drive run index` | each workspace package's `package.index`, after a change to a package |
| `cargo drive gate` | `fmt`, the lints, the build that ships, the Windows lint on Linux, the tests, the LAN check and each bench case once, to the first failure; CI runs `gate --ci`, which checks the format and runs every step |
| `cargo drive package` | the release build of the shipped binaries, stripped, each with its debug info beside it in `target/package` |

A step runs again only when one of its inputs changed since it passed, and the tests only for the crates a change reaches. `CAMPFIRE_BLESS=1` before `cargo drive test` writes the tests' pinned files again, `CAMPFIRE_BLESS=1 cargo drive test <test>` those of one test; before `cargo drive run zero-hour`, the check's golden file.

Each step prints one line; a failed step names its log, `target/drive/log/<step>.log`, which holds no passing test. A run ends on one verdict line: `ok`, or `FAIL` with the failed steps and the first error, and `target/drive/last-<command>.txt`, which holds all the run printed; above it, each failed step shows its first lines again. So `| tail -1` is enough: on a failure, read the file it names, rather than run the command again or grep the logs.

## Code

- **Banned calls go through their proxy crate**: files and threads through `store`, the formats through `codec`, schedule settings through `schedule`. Only a proxy's crate root `expect`s `clippy::disallowed_methods` or `clippy::disallowed_types`, but a `thread_local!` an outside API forces, whose module `expect`s `clippy::disallowed_types` at its head. When a proxy lacks what a caller needs, the proxy gains it.
- **Each operation has one home, by what it acts on:** the whole match a trait on `World`, the match's content an associated function of its module's type, a value made from the world a constructor of its own type, a test fixture a struct of the ids it places.
- **Native only.** Linux, macOS and Windows: no browser and no WebAssembly, and nothing that exists only for them.
- **No data in strings.** A fixed set is an enum, a value with rules a checked newtype, an error an enum of cases, each made where its text enters.
- **All file I/O goes through `store`**, in production code, tests and checks alike: no `std::fs`, no `Path::exists`, no `tempfile`. A read names its bound, and no code asks whether a path exists: it reads or lists, and takes `Missing`.
- **Game-bound code lives in a module named after its game**, `zero_hour`, in the crate whose interface it serves.
- **Tests run on nextest.** A new rule of a mode joins its scripted match, in a module named `scripted_match`, the one test exempt from the bound of 1 s.
- **Benches run only when the user's task asks for them.** A case's id is `<kind>/<group>/<case>`, its code a `bench.rs` beside what it measures, behind `internals`; `cargo drive bench --exact <id>` runs one, `--list` lists them.
