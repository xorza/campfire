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
        - `09-determinism-core.md` — Stage 1 proposal: numbers, vectors, randomness, stable ids, state hash
        - `10-sessions.md` — Stage 6 proposal: journal, crash restore, slots, reconnect, server bots, receipts, checkpoints, saves, local server
        - `11-rts-foundations.md` — box bodies, package unit types, group orders, production's first cut
        - `12-zero-hour.md` — the first imported game: importer, rules package, parity oracle, presentation
        - `13-bodies-uses-shop-gravity.md` — bodies up to 2,048 m and the spatial indexes' levels, the `use` action kind, the client's shop and item keys, projectiles that fall
    - `research/` — what a system's design is taken from: measurements and the sources read, facts only; `zero-hour.md` for the import
- `.notes/ISSUES.md` — the issue log: every open problem, under the step it waits for ([Issue log](#issue-log))
- `source/` — engine and game code; `source/packages/test/` holds the packages the tests play, the MOBA test content (heroes, spells, modes) in `test/moba/` among them; `source/packages/<game>/` holds an imported game's rules package
- `../GeneralsX/` — beside this repository, not in it: a checkout of [GeneralsX](https://github.com/fbraz3/GeneralsX), the released Generals and Zero Hour source with TheSuperHackers' fixes, built for Linux and macOS. It is the reference for Zero Hour's formats and rules, and the base of the parity oracle. It is GPL: read it as a specification, and copy no line of it ([Zero Hour](docs/design/12-zero-hour.md#decisions), D2).

`ROADMAP.md` holds what each milestone needs, in stages; `PLAN.md` holds the steps of the current slice. Both list open items only.

When code and design disagree, fix one of them, and state which one.

## Workflow

Work goes in vertical slices. A slice ends with something new that the user plays or sees in the client: a hero that walks its lane, a tower that shoots, a match that ends with a winner. A slice crosses every system it needs, and takes each one only as deep as the slice needs. It takes its items from any stage of the roadmap. A slice goes in four steps, each agreed before the next starts:

1. **Decide** the slice: what the player does or sees at its end, in one or two sentences. The issue log lists the problems the systems it touches already have.
2. **Investigate** the code it touches, and how established engines and games solve the problem and why.
3. **Propose** a design of what the slice adds, written into `docs/design/`, with an implementation plan in `PLAN.md`, for review. The plan names every system the slice changes.
4. **Implement** the plan as reviewed, step by step, each step ending with the checks passing. The slice ends when the user plays it in the client, and a scripted match keeps it working.

No changes to systems outside the plan along the way. A problem found outside the slice, or depth that the slice does not need, goes to the issue log, and a later slice takes it.

## Issue log

The issue log is `.notes/ISSUES.md`, one file for every open problem of the project.

- **System.** Each item names, in bold, the system whose code or data its fix changes: an engine module of [Modules](docs/design/02-engine-core.md#modules), the core or a capability of `docs/design/04-capabilities/`, the MOBA test content, or an imported game. When that system is not known, it names the system where the problem shows, and the item goes under Research.
- **Sections.** The file has three sections, in this order, and an item sits in the section of its next step:
    - **Decide**: the options are known, and the user chooses one. The item names the question and its options.
    - **Research**: the cause, or the right fix, is not known.
    - **Ready**: the fix is known, and waits for its turn.
- **Items.** One bullet for each problem: its system, then what is wrong, where the code and the design differ, and not how to fix it. An item that a roadmap stage or a plan step takes carries its tag after its system, `**Client.** **Stage 5.**` or `**Net.** **Plan: F3.**`; an item with no tag has no stage yet. Within a section, items go in the order of their systems' names.
- **Life.** An item moves between sections as it is triaged. A fixed item is deleted, with no done marker and no history. A section with no items stays, empty.

## Code

- **No data in strings.** A value from a fixed set is an enum, a value with rules is a checked newtype, and an error is an enum of cases. Text from data files, scripts, JSON or the network becomes these types where it enters. Strings stay only for human text and for names the outside format defines.
- **All file I/O goes through `store`**, as [Storage](docs/design/02-engine-core.md#storage) says: every read, listing, check and write of a file, in production code, tests and checks alike. Production code uses its types; a test uses its internals for every file it makes or reads. No `std::fs`, no `Path::exists` and its kin, no `tempfile`, and no `expect` or `allow` of `clippy::disallowed_methods` or `clippy::disallowed_types` outside `store`: when `store` lacks what a caller needs, `store` gains it. A read names its bound, and no code asks whether a path exists: it reads or lists, and takes `Missing`.
- **Game-bound code lives in a module named after its game**, `zero_hour`, in the crate whose interface it serves, so no reader takes it for a general system; nothing but the place that installs it depends on it ([Modules](docs/design/02-engine-core.md#modules)).
- **Benches run only when the user's task asks for them.** Optimization has its own sessions, which the user starts; until then a step's checks are its tests, and a bench case a step adds is built by clippy, not run.
- **Benches run through `cargo benches`**, never `cargo bench` with `--workspace` or one `-p` ([Benches](docs/design/02-engine-core.md#benches)). A crate that gains a `[[bench]]` joins the alias in `source/.cargo/config.toml`. One case: `cargo benches -- --exact atomic/num/mul`; a kind or a group: `-- '^atomic/'`, `-- atomic/num/`; one crate: `--bench math`; the ids: `-- --list`.
- **Bench structure.** An id is `<kind>/<group>/<case>`: `atomic` for one operation over inputs, `integration` for one system's tick on a made scene or a whole end on the test packages. The group is the path measured, unique in the workspace; the case is the operation, the scene, or the statistic and scenario (`worst_3v3`). Benches live in a `bench.rs` beside their code, gated `#[cfg(feature = "bench")]`, called from `lib.rs`'s `bench::run`. A fixture that reads files or runs ticks is made in the first case that needs it, so a filter that skips the case costs nothing.
- **Match tests** follow [One scripted match for each mode](docs/design/02-engine-core.md#testing-and-diagnostics): a new rule joins the mode's scripted match, not a new run. A mode's scripted match test is the one test exempt from the bound of 1 s for a test; every other test keeps it.
