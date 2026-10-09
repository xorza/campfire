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
  - `issues/` — the issue log: one file for each system, with its open problems ([Issue log](#issue-log))
- `source/` — engine and game code; `source/packages/test/` holds the packages the tests play, the MOBA test content (heroes, spells, modes) in `test/moba/` among them; `source/packages/<game>/` holds an imported game's rules package

`ROADMAP.md` holds what each milestone needs, in stages; `PLAN.md` holds the steps of the current slice. Both list open items only.

When code and design disagree, fix one of them, and state which one.

## Workflow

Work goes in vertical slices. A slice ends with something new that the user plays or sees in the client: a hero that walks its lane, a tower that shoots, a match that ends with a winner. A slice crosses every system it needs, and takes each one only as deep as the slice needs. It takes its items from any stage of the roadmap. A slice goes in four steps, each agreed before the next starts:

1. **Decide** the slice: what the player does or sees at its end, in one or two sentences. The issue log's files of the systems it touches list the problems they already have.
2. **Investigate** the code it touches, and how established engines and games solve the problem and why.
3. **Propose** a design of what the slice adds, written into `docs/design/`, with an implementation plan in `PLAN.md`, for review. The plan names every system the slice changes.
4. **Implement** the plan as reviewed, step by step, each step ending with the checks passing. The slice ends when the user plays it in the client, and a scripted match keeps it working.

No changes to systems outside the plan along the way. A problem found outside the slice, or depth that the slice does not need, goes to the issue log, and a later slice takes it.

## Issue log

The issue log is `docs/issues/`, one file for each system: each engine module of [Modules](docs/design/02-engine-core.md#modules), the core and each capability of `docs/design/04-capabilities/`, the MOBA test content, and each imported game. `docs/issues/README.md` lists the files. A new module or capability gets its file when the design adds it.

- **Where.** A problem goes to the file of the system whose code or data its fix changes, not of the system where it shows. When that system is not known, it goes to the file of the system where it shows, under Research.
- **Sections.** Each file has three sections, in this order, and an item sits in the section of its next step:
  - **Decide**: the options are known, and the user chooses one. The item names the question and its options.
  - **Research**: the cause, or the right fix, is not known.
  - **Ready**: the fix is known, and waits for its turn.
- **Items.** One bullet for each problem: what is wrong, where the code and the design differ, and not how to fix it. An item that a roadmap stage or a plan step takes starts with its tag, **Stage 5.** or **Plan: F3.**; an item with no tag has no stage yet.
- **Life.** An item moves between sections as it is triaged. A fixed item is deleted, with no done marker and no history. A section with no items stays, empty.

## Code

- **No data in strings.** A value from a fixed set is an enum, a value with rules is a checked newtype, and an error is an enum of cases. Text from data files, scripts, JSON or the network becomes these types where it enters. Strings stay only for human text and for names the outside format defines.
- **All file I/O goes through `store`**, as [Storage](docs/design/02-engine-core.md#storage) says: every read, listing, check and write of a file, in production code, tests and checks alike. Production code uses its types; a test uses its internals for every file it makes or reads. No `std::fs`, no `Path::exists` and its kin, no `tempfile`, and no `expect` or `allow` of `clippy::disallowed_methods` or `clippy::disallowed_types` outside `store`: when `store` lacks what a caller needs, `store` gains it. A read names its bound, and no code asks whether a path exists: it reads or lists, and takes `Missing`.
- **Game-bound code lives in a module named after its game**, `zero_hour`, in the crate whose interface it serves, so no reader takes it for a general system; nothing but the place that installs it depends on it ([Modules](docs/design/02-engine-core.md#modules)).
- **Benches run through `cargo benches`**, never `cargo bench` with `--workspace` or one `-p`, so every case builds from the same features ([One selection for every run](docs/design/02-engine-core.md#benches)): `--bench <crate>` takes one crate, `-- <filter>` a group. A crate that gains a `[[bench]]` joins the alias in `source/.cargo/config.toml`.
- **Match tests** follow [One scripted match for each mode](docs/design/02-engine-core.md#testing-and-diagnostics): a new rule joins the mode's scripted match, not a new run. A mode's scripted match test is the one test exempt from the bound of 1 s for a test; every other test keeps it.
