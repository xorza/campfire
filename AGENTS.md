# Campfire

An open-source (MIT/Apache-2.0) Rust engine for multiplayer games, plus a reference 3v3 MOBA. The sim is deterministic and server-authoritative: every session log can be replayed and verified. Game content is Rhai scripts and data in signed packages. Identity, server discovery and the marketplace use Nostr. Lightning payments are an optional module.

## Layout

- `docs/` — the project's documents:
  - `design/` — the design, and the source of truth for every decision. Read it before you change anything:
    - `01-campfire-design.md` — vision, terms, principles, milestones
    - `02-engine-core.md` — modules, determinism rules, Bevy and Lightyear, libraries
    - `03-game-scripting.md` — packages, tick pipeline, script state, numbers, network sync
    - `04-capabilities/` — the mechanisms the engine provides (combat, commands, character, navigation, vision, …), which a game combines, and the genres they make
    - `05-protocol-spec.md` — keys, connection, session log, verification, Nostr events, payments
    - `06-research-notes.md` — early decisions, risks and sources
    - `07-reference-moba.md` — reference MOBA: heroes, player spells, rules
    - `08-script-api.md` — script API by capability, derived from the reference packages
    - `08-script-api-reference.md` — every name of the script API, with its roles, capability and whether it runs; generated from the registry by a test, never edited by hand
    - `09-determinism-core.md` — Stage 1 proposal: numbers, vectors, randomness, stable ids, state hash
    - `10-sessions.md` — Stage 6 proposal: journal, crash restore, slots, reconnect, server bots, receipts, checkpoints, saves, local server
    - `11-storage.md` — proposal: one crate for durable files and worker threads, one data layout, one fault surface, and a session log with no IO
    - `12-structure.md` — proposal: one rule for each kind of value, one place for each error, one name for each meaning, and one copy of each piece of wiring
  - `issues/` — the issue log: one file for each system, with its open problems ([Issue log](#issue-log))
- `source/` — engine and game code; `source/packages/<game>/` holds the reference content packages (heroes, spells, modes); `source/packages/test/` holds small packages the tests play

`ROADMAP.md` holds the milestone stages; `PLAN.md` holds the next concrete steps. Both list open items only.

When code and design disagree, fix one of them, and state which one.

## Workflow

Work goes system by system, in four steps, each agreed before the next starts:

1. **Decide** what to work on: one system, or one section of `PLAN.md`. The system's file in the issue log lists the problems it already has.
2. **Investigate** the code it touches, and how established engines and games solve the problem and why.
3. **Propose** a design, written into `docs/design/`, with an implementation plan in `PLAN.md`, for review.
4. **Implement** the plan as reviewed, step by step, each step ending with the checks passing.

No small, unplanned changes to other systems along the way. A problem found outside the system goes to the issue log, and is planned with its own system later.

## Issue log

The issue log is `docs/issues/`, one file for each system: each engine module of [Modules](docs/design/02-engine-core.md#modules), the core and each capability of `docs/design/04-capabilities/`, and the reference MOBA. `docs/issues/README.md` lists the files. A new module or capability gets its file when the design adds it.

- **Where.** A problem goes to the file of the system whose code or data its fix changes, not of the system where it shows. When that system is not known, it goes to the file of the system where it shows, under Research.
- **Sections.** Each file has three sections, in this order, and an item sits in the section of its next step:
  - **Decide**: the options are known, and the user chooses one. The item names the question and its options.
  - **Research**: the cause, or the right fix, is not known.
  - **Ready**: the fix is known, and waits for its turn.
- **Items.** One bullet for each problem: what is wrong, where the code and the design differ, and not how to fix it. An item that a roadmap stage or a plan step takes starts with its tag, **Stage 5.** or **Plan: F3.**; an item with no tag has no stage yet.
- **Life.** An item moves between sections as it is triaged. A fixed item is deleted, with no done marker and no history. A section with no items stays, empty.

## Code

- **No data in strings.** A value from a fixed set is an enum, a value with rules is a checked newtype, and an error is an enum of cases. Text from data files, scripts, JSON or the network becomes these types where it enters. Strings stay only for human text and for names the outside format defines.
- **Match tests** follow [One scripted match for each mode](docs/design/02-engine-core.md#testing-and-diagnostics): a new rule joins the mode's scripted match, not a new run.
