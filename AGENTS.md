# Campfire

An open-source (MIT/Apache-2.0) Rust engine for multiplayer games, plus a reference 3v3 MOBA. The sim is deterministic and server-authoritative: every session log can be replayed and verified. Game content is Rhai scripts and data in signed packages. Identity, server discovery and the marketplace use Nostr. Lightning payments are an optional module.

## Layout

- `design/` — the design, and the source of truth for every decision. Read it before you change anything:
  - `01-campfire-design.md` — vision, terms, principles, milestones
  - `02-engine-core.md` — modules, determinism rules, Bevy and Lightyear, libraries
  - `03-game-scripting.md` — packages, tick pipeline, script state, numbers, network sync
  - `04-game-kits/` — genre kits: MOBA first, FPS, MMO and battle royale later
  - `05-protocol-spec.md` — keys, connection, session log, verification, Nostr events, payments
  - `06-research-notes.md` — early decisions, risks and sources
  - `07-reference-moba.md` — reference MOBA: heroes, player spells, rules
  - `08-script-api.md` — script API of the MOBA kit, derived from the reference packages
  - `09-determinism-core.md` — Stage 1 proposal: numbers, vectors, randomness, stable ids, state hash
- `source/` — engine and game code; `source/packages/<kit>/` holds the reference content packages (heroes, spells, modes)

`ROADMAP.md` holds the milestone stages; `PLAN.md` holds the next concrete steps. Both list open items only.

When code and design disagree, fix one of them, and state which one.
