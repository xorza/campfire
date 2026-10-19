# Workspace audit A: Campfire's crates

Date: 2026-10-19. Scope: `source/crates/{base,sim,capabilities,session,apps,test}/**` and `source/checks/*`, 50 crates. Not audited: `campfire-net`, `campfire-quic` and the 22 `campfire-net-*` crates; this audit only notes where they connect to these crates.

How the facts were gathered:
- `cargo metadata --no-deps` gave the manifests and `cargo tree --offline -e normal` gave each crate's required closure.
- A scan of each crate's sources split every dependency's mentions into four kinds of code: production, `internals`-gated, `#[cfg(test)]`, and `tests/`+`benches/`. Every flag the scan raised was checked by hand. The scan's false positives (code that `#[path]` or `#[cfg(unix)]` selects, and gated enum variants) are dropped from this report.
- Layers are computed as `1 + max(layer of each normal dependency)`, which is the rule in `02-engine-core.md`.

What I could not check: nothing was compiled, so the claim that a feature or dependency can go is based on the absence of any name in the sources and on Cargo's feature unification, not on a build. A dependency used only through method resolution (an `impl` in a dependency's `internals`, called without naming its crate) is invisible to a name search. Where that matters, the finding marks it **unverified**.

Kinds, as `13-net-stack/layering.md` defines them:
- **P**, problem crate: one problem, and no Bevy crate other than `bevy_platform` among its required dependencies.
- **G**, glue crate: binds problem crates to the ECS, or holds a problem whose nouns are entities and components.
- **App**: a shipped binary.
- **Check**: an unshipped binary.
- **T**: test support.

## Crates

"Req" is the number of required (non-optional, normal) dependencies, with the Campfire ones in parentheses. "Bevy" says whether a Bevy engine crate is in the required closure, and gives the closure's size in crates.

| Path | Name | Kind | L | Responsibility | Req | Bevy (closure) | Not needed / could narrow |
| --- | --- | --- | --- | --- | --- | --- | --- |
| base/codec | codec | P (proxy) | 0 | The one serde gateway: postcard, TOML, JSON | 6 (0) | no (35) | none |
| base/common-derive | common-derive | P | 0 | `NamedEnum`/`EnumAll`/`EnumIdents` derives | 3 (0) | no (7) | none |
| base/common | common | P | 1 | Shared plain values, hashers, `ErrorReport`/`ExitStatus`; under `internals`, the allocation audit | 6 (1) | no (32) | The allocation audit is test support (A9) |
| base/store | store | P (proxy) | 0 | Files, directories, workers, stop signals, `SharedPort`; under `internals`, test directories and pins, plus a Rust-source and Cargo-manifest reader | 6 (0) | no (15) | `SourceFiles` and `CargoManifest` (A10) |
| base/log | log | P | 2 | Typed log events; the `start` feature sets up a binary's subscriber | 4 (2) | no (63) | none |
| base/schedule | schedule | G (proxy) | 0 | The sim schedules' build settings | 1 (0) | bevy_ecs (58) | none; it is placed in `base` (A18) |
| sim/math | math | P | 2 | Fixed-point numbers, vectors, trig, RNG | 4 (1) | no (38) | none |
| sim/sim | sim | G | 3 | The deterministic ECS core: stable ids, ticks, state registry, schedule | 9 (4) | yes (100) | none |
| capabilities/core/geometry | geometry | P | 3 | Exact geometry | 3 (1) | no (40) | none; it is placed with the ECS core crates (A18) |
| capabilities/core/state-types | state-types | G | 4 | The traits by which state types register and send; also two plain types | 3 (2) | yes (101) | `DataKind`, `Recipients` (A4) |
| capabilities/core/values | values | G (mixed) | 5 | Plain data values, names and ids, plus 4 identity components | 8 (4) | yes (104) | `sim`, `state-types`, `bevy_ecs` serve only 6 of its 47 files (A2) |
| capabilities/core/visibility | visibility | G (barely) | 5 | Replication audiences, as a mode's data and a host's settings state them | 5 (2) | yes (102) | `bevy_ecs` serves one `Resource` derive; `state-types` serves two plain types (A4) |
| capabilities/core/script | script | G | 6 | The Rhai host, the script API's registry, the script view | 11 (6) | yes (124) | none (A5) |
| capabilities/core/declare | declare | G | 7 | How a module declares what it adds | 11 (8) | yes | none |
| capabilities/core/units | units | G | 8 | The unit model, the map's book, the ground, the contracts | 11 (8) | yes | none |
| capabilities/core/players | players | G | 9 | The match's and players' entities, resources, commands | 11 (9) | yes | none |
| capabilities/core/actions | actions | G | 10 | The action pipeline and slots | 14 (11) | yes | none |
| capabilities/core/deliveries | deliveries | G | 11 | Actions' deliveries and their hooks | 11 (9) | yes | none |
| capabilities/{abilities, areas, combat, interaction, items, navigation, production, progression, projectiles, stats, vision} | each capability | G | 10–12 | One capability each | 11–17 | yes | navigation's dev-dependency on `codec` (A14) |
| capabilities/mode | mode | G | 13 | The mode: script, state, choices, spawns | 20 (17) | yes | none (combat, navigation, production and progression are `USES`, as declared) |
| capabilities/orders | orders | G | 13 | The orders capability | 17 (14) | yes | none |
| capabilities/capabilities | capabilities | G (composition root) | 14 | The release, the books, `Match`, `Order`/`Action`; 22.6k lines of whole-match tests | 28 (25) | yes | none; a structural test reads bots' sources (A11) |
| session/protocol | protocol | P | 2 | Signatures, keys, handshake, chains, session log; no IO | 8 (2) | no (113; `nostr` brings 70) | none |
| session/client-files | client-files | G only because of `values` | 6 | The client's files of a package | 6 (3) | yes (112) | Names only `PackagePath` and `Team` of `values` (A3) |
| session/package | package | G | 15 | Package files and schemas, plus the load check against the release | 16 (10) | yes (176) | none; holds two parts (A6) |
| session/package-disk | package-disk | G (IO) | 16 | Package directories and the store on disk | 5 (4) | yes | none |
| session/bots | bots | G | 15 | Bots' order scripts, resolved against the books and the world | 18 (15) | yes (168) | dev `campfire-package[internals]` named nowhere (A15) |
| session/runner | runner | G | 16 | Builds and feeds a match; replays a log | 16 (11) | yes (236) | none; dev-depends upward (A12) |
| session/verifier | verifier | App + lib | 17 | Replays and checks a session log (the lib is 298 lines) | 12 (8) | yes (267) | none; placed in `session/` (A18) |
| apps/import | import | App + lib | 17 | Writes packages from a game's install (21.8k lines) | 25 (18) | bevy_ecs, bevy_math (223) | none; holds three parts (A20) |
| apps/client | client | App (bin only) | 18 | The Bevy client (12.7k lines) | 42 (32) | full `bevy` | none; has no library (A19) |
| apps/server | server | App | 18 | The dedicated server's binary (958 lines) | 19 (13) | yes | none |
| test/harness | harness | T | 18 | Match and session fixtures, goldens, and the in-process network match | 42 (35) | yes (478, with lightyear and quic) | `areas`, `projectiles`, `state-types` belong in dev-dependencies (A8); network part could split off (A7) |
| test/rules | rules | T | 0 | The structural rules as tests | 0 (0) | no | none |
| test/test-packages | test-packages | T | 17 | The workspace's test packages | 5 (5) | yes (240) | none |
| checks/lan-check | lan-check | Check | 18 | Real server and bots over QUIC on localhost | 13 (9) | full `bevy` (450) | none |
| checks/vocabulary | vocabulary | Check | 3 | Writes the repository's layout and names | 9 (3) | no (95) | none |
| checks/zero-hour-check | zero-hour-check | Check | 19 | Imports and plays a Zero Hour install | 32 (27) | yes (487) | Pulls `net`, `quic`, `lightyear` and `criterion` through `harness` (A7) |

## Layer graph as it is now

Layers come from normal dependencies only. Net crates are shown only where they interleave.

```
L0  codec  common-derive  schedule  store  rules            (net-common net-despawn net-payload)
L1  common                                                    (net-bytes)
L2  log  math  protocol                                       (net-core net-port-map quic)
L3  geometry  sim  vocabulary
L4  state-types
L5  values  visibility
L6  client-files  script
L7  declare
L8  units
L9  players
L10 actions  vision
L11 deliveries  interaction  stats
L12 abilities  areas  combat  items  navigation  production  progression  projectiles
L13 mode  orders
L14 capabilities
L15 bots  package
L16 package-disk  runner
L17 import  test-packages  verifier                           (net)
L18 client  harness  lan-check  server
L19 zero-hour-check
```

No normal edge goes up a layer or stays within one; Cargo and the layer test hold that. The edges that look wrong:

- **values (L5) → sim, state-types, bevy_ecs.** These edges exist for four components and one id type (A2). They lift client-files and every reader of plain values onto the ECS.
- **client-files (L6) → values.** The crate names two items of `values` and none of the ECS (A3).
- **visibility (L5) → state-types.** It needs two plain types, `DataKind` and `Recipients`, and no trait (A4).
- **zero-hour-check (L19) → harness.** It uses three non-network fixtures but takes on the whole network harness (A7).
- **Dev edges upward:** bots (L15) and runner (L16) dev-depend on test-packages (L17) (A12).
- **A test reading upward by path:** capabilities (L14) reads `session/bots/src` (A11).
- **A test writing into another crate:** import (L17) writes `client/tests/fixtures` (A13).

## Problem crates today, and those one move away

- **Already engine-free:** codec, common, common-derive, store, log, math, geometry, protocol, plus the vocabulary binary. No rule holds them so (A1).
- **values:** would be engine-free if its four components and the `StableId` fields moved behind an `ecs` feature (A2).
- **visibility:** would be engine-free if `DataKind` and `Recipients` moved into it and `OutView`'s `Resource` derive went behind `ecs` (A4).
- **client-files:** would become a problem crate after A2, except that it requires `bevy_math` (A3).
- **package's format part:** files, names, index and locales; it is not separable today without splitting the crate (A6).
- **script's Rhai host:** 1,108 lines, engine-free but for one `Resource` (A5).
- **import's Zero Hour format readers:** 9.1k lines, engine-free but for two imports (A20).

## Findings

### Problem and glue crates across the engine

**A1. The problem/glue split covers the network stack only.** Confidence: high.
- *Evidence:*
  - The Kinds decisions live in `docs/design/13-net-stack/layering.md:8-25`. `02-engine-core.md`'s modules table (lines 7-84) and its Layers decision (line 89) do not classify crates.
  - `PROBLEM_CRATES` in `crates/test/rules/src/manifests.rs:113-122` lists the 8 `net-*` problem crates and nothing else.
  - Eight Campfire library crates are engine-free today: codec, common, common-derive, store, log, math, geometry, protocol (`cargo tree`, above). Nothing stops a Bevy crate from entering their closure.
- *Why it matters:* the user's goal is problem crates and glue crates across the whole workspace. The design and the rule only state it for the stack.
- *Direction:*
  - Move the Kinds decisions into `02-engine-core.md`, so they apply engine-wide.
  - Add the eight crates to `PROBLEM_CRATES`, or derive the list from a mark in each manifest so the modules table and the rule cannot drift apart.

**A2. `values` holds plain values and four ECS components.** Confidence: high for the facts, medium for the direction.
- *Evidence:*
  - 41 of `values`' 47 source files are plain.
  - Four components need the ECS: `Team` (`team.rs:1-5`), `UnitType` (`unit_type.rs:1-5`), `UnitTags` (`unit_tags.rs:1`) and `Relations` (`relations.rs:3-7`). Each derives `Component` and implements `SimComponent` and `Replication`.
  - `Hit` (`hit.rs:2`) and `DashDelivery` (`dash_delivery.rs:1`) hold `campfire_sim::StableId`.
  - These six files are the only uses of `bevy_ecs`, `campfire-sim` and `campfire-state-types` in the crate. Because of them, the required closure is 104 crates; with `common` and `math` alone it would be about 40, since `geometry`, which has the same base, is 40.
  - The crate doc (`values/src/lib.rs:1-4`) names both jobs. The design table's description ("Data value types, names and ids") names only the first.
- *Why it matters:* clean boundaries and minimal dependencies. Every consumer of a package path or a name pays for the ECS core, `client-files` among them (A3).
- *Direction:*
  - Apply `layering.md`'s own decision "A plain type the ECS holds takes its ECS derives behind its crate's features" (line 19). The four components would put their `Component` derive and their `SimComponent`/`Replication` impls behind an `ecs` feature.
  - `StableId` needs a plain home whose `Component` hooks `sim` registers.
  - Alternatively, the four components move to a small glue crate under `script`.

**A3. `client-files` is bound to the ECS through two names.** Confidence: high.
- *Evidence:*
  - Its only imports from `values` are `PackagePath` and `Team` (`grep` across `client-files/src`). It names no `bevy_ecs` item, yet its required closure is 112 crates, `bevy_ecs` among them.
  - It also requires `bevy_math` (used in 4 files). That follows `02-engine-core.md`'s Libraries row "Client files' vectors: `bevy_math`".
  - The rules' `is_engine` (`manifests.rs:126-128`) counts `bevy_math` as engine, so after A2 the crate would still fail the problem-crate rule.
- *Why it matters:* this is a format crate that the sim never reads; it should build without the sim.
- *Direction:* after A2, decide whether `bevy_math` (no ECS, only math types) is admitted beside `bevy_platform` in problem crates. If not, keep `client-files` as a known exception in the design.

**A4. `state-types` holds two plain types, and `visibility` needs only those.** Confidence: high for the facts, medium for the direction.
- *Evidence:*
  - `DataKind` (`state-types/src/data_kind.rs`) and `Recipients` (`recipients.rs`) derive no ECS trait.
  - `visibility` imports exactly those two from `state-types` (`setting/mod.rs:5`, `setting/error.rs:1`, `visibility.rs:2`, `error.rs:1`).
  - Its only ECS item is `OutView`'s `Resource` derive (`out_view.rs:1`). That gives a closure of 102 crates for a crate whose doc says it carries "the bytes a session's terms carry".
- *Why it matters:*
  - Audiences are replication vocabulary. The net stack's replication glue and the session's terms both read it.
  - As a problem crate, `visibility` would also serve `protocol`-level code without the ECS.
- *Direction:*
  - Move `DataKind` and `Recipients` into `visibility`, so that `state-types` depends on `visibility`.
  - Put `OutView`'s `Resource` derive behind an `ecs` feature. `visibility` then becomes a problem crate at layer 2.

**A5. `script`'s Rhai host is nearly engine-free.** Confidence: low.
- *Evidence:*
  - `script_host/` is 1,108 lines; its only ECS item is `ScriptPrints`' `Resource` derive (`script_host/script_prints.rs:3`).
  - `scripts/` (9,238 lines) holds the registry and the view, which are ECS-bound.
- *Why it matters:* a sandboxed interpreter host is a problem in itself. However, `02-engine-core.md:220`, "Rhai, not `bevy_mod_scripting`", decides that `script` alone names Rhai.
- *Direction:* leave it as it is unless a second user of the host appears; splitting the host out would change that decision.

**A6. `package` holds the package format and the load check against the release.** Confidence: medium for the facts, low for the direction.
- *Evidence:*
  - 21 of `package`'s 42 non-test source files name `capabilities`, `mode`, `actions`, `units`, `declare`, `script` or `sim`, for example `mode_packages/load_check.rs` and `files/units_file.rs`.
  - The format part needs none of them: `file_index`, `package_walk`, `package_name`, `language`, `locale_*` and `texts`.
  - Its closure is 176 crates, with 31 Campfire crates among them.
- *Why it matters:* one responsibility per crate.
- *Direction:* a `package-format` problem crate becomes worth it only when a reader that needs the format without the release exists (a launcher, or a content signer). `launcher` and `content` are planned in the modules table. Until then, record the boundary.

### Test support

**A7. `harness` is two harnesses, and `zero-hour-check` pays for both.** Confidence: high for the facts, medium for the direction.
- *Evidence:*
  - Only `src/in_process_match/` (1,315 lines) and `src/bench/` name `campfire-net`, `lightyear`, `bevy_app`, `bevy_state` or `bevy_time`. The other 2.2k lines of `src/` (fixed sessions, goldens, `Moba3v3`, the proving match, `MatchUnits`) do not.
  - `zero-hour-check` uses only `FixedSession`, `FixedMatch` and `MatchUnits` (`skirmish_play.rs:7`, `start_drive.rs:5`, `match_play.rs:4`). Even so, its closure is 487 crates, with `lightyear`, `bevy_replicon`, `quinn-proto`, `campfire-net`, `campfire-quic` and `criterion` among them. `runner`'s closure is 236.
- *Why it matters:*
  - The user asked for glue crates that test integrated functionality, with minimal dependencies.
  - Plan batch "Campfire on the stack" rewrites `in_process_match` onto the stack's stepper and emulator anyway (`PLAN.md`, `source/crates/test/harness/src/in_process_match/`).
- *Direction:* split the crate into two:
  - a session harness (runner, packages and goldens, no network), which `zero-hour-check` and the session tests use;
  - a network harness that glues the session harness to `net-stack`'s stepper.

  Doing it in batch "Campfire on the stack" avoids writing the network part twice.

**A8. `harness` requires three crates that only its tests read.** Confidence: high.
- *Evidence:*
  - `harness/Cargo.toml:15,35,41` makes `campfire-areas`, `campfire-projectiles` and `campfire-state-types` required.
  - They are named only in `tests/`: areas in `tests/package/mode_package.rs`, `tests/runner/{proving_match,moba_abilities}.rs`; projectiles in those three plus `tests/net/prototype.rs` and `tests/verifier.rs`; state-types in `tests/net/leak.rs`.
  - The manifest's own comment (line 12) says it "dev-depends so on each crate that only its tests read", as it does for `deliveries`, `interaction` and `verifier`.
  - The rule `a_required_dependency_is_named_by_production_code` does not catch this, because `code_by_crate` counts every file of `TOP_TEST_CRATE` as production (`rules/src/manifests.rs:246,565`).
- *Why it matters:* the stated rule, and minimal dependencies. A7's split would also carry these three edges along needlessly.
- *Direction:* move the three to `[dev-dependencies]`. Make the rule read only `harness`'s `src/` as production, so the exemption no longer hides this.

**A9. Three counting allocators.** Confidence: high for the facts, medium for the direction.
- *Evidence:*
  - `common/src/alloc_audit/counting_allocator.rs:23`: windows over the whole process, with backtraces. Used by `common/tests/alloc` (its self-test) and by `network/quic/tests/alloc`.
  - `harness/src/bench/allocations.rs:25`: counts everything, for criterion.
  - `network/net-stack/tests/alloc/allocator/mod.rs:20`: per-thread counters, with backtraces.
  - Plan batch "Campfire on the stack" deletes `quic`. After that, `common`'s audit has no user but its own self-test.
- *Why it matters:*
  - Duplicate helpers.
  - `common`'s rule (`02-engine-core.md:93`, "`common` holds only shared plain values") is bent to host the audit, together with `backtrace` and two `thread_local!`s.
- *Direction:* one allocation-audit crate (test support, engine-free, `backtrace` only) that serves `net-stack`'s alloc tests, `harness`'s allocation bench and any crate's `tests/alloc`, with the per-thread and per-process window as its two modes. `common` then drops its `internals` audit and the `backtrace` dependency.

**A10. `store`'s `internals` parses Rust sources and Cargo manifests.** Confidence: medium.
- *Evidence:*
  - `store/src/source_files.rs` (406 lines) scans `#[cfg(test)]` and `internals` gates in Rust text.
  - `store/src/cargo_manifest.rs` (119 lines) parses manifests "by hand, as `store` has no TOML parser" (line 22), while `codec` holds the TOML parser.
  - Users: `rules` (7 files) and `capabilities/src/release/tests/mod.rs`.
  - Separately, `checks/vocabulary` reads Rust sources with `syn`.
- *Why it matters:* `store`'s decision is "owns every file and thread" (`02-engine-core.md`, Storage), while this code parses text. A proxy crate then carries test-only parsing that every crate's test build compiles.
- *Direction:* move both readers into `rules`, reading files through `store`'s `InputFile` and manifests through `codec`'s TOML. That also removes the hand parser.

**A11. A structural rule lives in `capabilities` and reads a higher crate's sources.** Confidence: medium.
- *Evidence:* `capabilities/src/release/tests/mod.rs:914` joins `CARGO_MANIFEST_DIR` with `../../session/bots/src` and walks bots' production code for the name-lookup allowlist.
- *Why it matters:*
  - `bots` is at L15, above `capabilities` at L14, so a lower crate's test depends on a higher crate's files.
  - Structural rules have one home, `rules` (`02-engine-core.md`, Structural rules: "Each rule has a test").
- *Direction:* move the name-lookup allowlist test into `rules`, which already reads every crate.

**A12. `bots` and `runner` dev-depend on a crate above them.** Confidence: high for the facts, medium for the direction.
- *Evidence:*
  - `bots/Cargo.toml:33` (bots is L15) and `runner/Cargo.toml:46` (runner is L16) dev-depend on `test-packages`, which is L17 because it requires `package-disk`.
  - Each uses it in one test: `bots/src/bot_script/tests.rs` and `runner/src/session_rules/tests.rs`.
  - `02-engine-core.md:99` decides "no crate dev-depends on a crate above it". No second copy of a crate arises today, but no rule checks this either.
- *Why it matters:* a written rule that the code does not hold.
- *Direction:* move the two tests into `harness`, or change the rule's text to the property it protects ("no test build holds a second copy of a crate") and add a test for it.

**A13. `import`'s test writes `client`'s fixtures.** Confidence: medium.
- *Evidence:* `import/src/texture/mod.rs:49-56` pins `dxt1.ktx2` and `rgba.ktx2` into `../client/tests/fixtures`.
- *Why it matters:* a crate's test writes another crate's tree. The comment explains why: the client depends on no import.
- *Direction:* acceptable as a pinned cross-check. If A10 moves workspace-reading into `rules`, a fixture crate or `test-packages` could hold the textures instead.

### Manifest hygiene

**A14. `navigation`'s dev-dependency on `codec` is unused and redundant.** Confidence: high.
- *Evidence:*
  - `navigation/Cargo.toml:42` declares `campfire-codec = { …, features = ["internals"] }`.
  - No file of `navigation` (`src`, `tests`, `benches`) names `campfire_codec`.
  - `codec`'s `internals` is already on through `campfire-sim/internals`.
- *Why it matters:* minimal dependencies.
- *Direction:* delete the line.

**A15. `bots` has no `internals` feature and a dev-dependency nothing names.** Confidence: medium (unverified: a method of `package`'s `internals`, `ModePackages::tag_names`, could be called without naming the crate; none of bots' tests calls it).
- *Evidence:*
  - `bots/Cargo.toml:30` declares `campfire-package = { …, features = ["internals"] }`; no bots code names `campfire_package`.
  - `bots` is the one library crate below the apps that has tests but no `internals` feature or self dev-dependency.
- *Why it matters:* consistency of the `internals` pattern.
- *Direction:* give `bots` the standard `internals` feature, forwarding to its dependencies, with the self dev-dependency, and drop the unnamed dev-dependency.

**A16. `#[cfg(test)]` modules named `internals`.** Confidence: medium.
- *Evidence:*
  - `import` holds 29 `#[cfg(test)] pub(crate) mod internals` modules, for example `gltf/glb_builder.rs:223` and `zero_hour/game_version.rs:251`; `sim` holds one (`stable_id/state_changes.rs:113`) and `client` one (`view/package_source/mod.rs:180`).
  - `import`'s `internals` feature gates no code of its own.
  - `02-engine-core.md:288` decides "`#[cfg(test)]` holds a crate's own test modules alone".
- *Why it matters:* one word, one meaning. A module named after the feature but gated by `test` reads as the other gate.
- *Direction:* either gate these helpers behind `internals`, or rename the modules. Also align the design's sentence with the user's rule, which allows `#[cfg(test)]` reach-ins that only the crate's own tests call.

**A17. The design text on `common`'s dependencies omits `zeroize`.** Confidence: high.
- *Evidence:*
  - `02-engine-core.md:159` and `common/src/lib.rs:12-13` list serde, derive_more, the hashers and backtrace.
  - The manifest requires `zeroize` (used by `secret.rs`), and the rule's list includes it (`rules/src/manifests.rs:28`).
- *Why it matters:* the design and the code disagree.
- *Direction:* add `zeroize` to both texts.

### Layout

**A18. Groups do not follow kind or layer.** Confidence: medium.
- *Evidence:*
  - `geometry` is engine-free at L3 with only `math` below it, yet sits in `capabilities/core/` among ECS crates.
  - `client-files` is L6 and depends on no session crate, yet sits in `session/`.
  - `schedule` is the only Bevy crate in `base/`; the rest of `base/` is engine-free.
  - `verifier` is a shipped binary in `session/`, while the other shipped binaries are in `apps/`.
- *Why it matters:* the user wants clean boundaries. A group that says what kind of crate it holds makes a misplaced dependency visible in review.
- *Direction:* if A1 is adopted, regroup around kinds and layers:
  - `geometry` beside `math` in `sim/`;
  - `schedule` beside `sim`;
  - `client-files` near `package`;
  - `verifier` in `apps/`.

  `02-engine-core.md:138`'s group rule still holds after these moves.

**A19. The client has no library.** Confidence: medium for the facts, low for the direction.
- *Evidence:*
  - `apps/client` is a single binary of 12,724 lines: `view/` is 6,127 lines and `hud/` 1,724.
  - No other crate can reach its code. `harness`'s `client_frame` bench measures `net`'s client app through `in_process_match`, not the client's view.
- *Why it matters:* the user wants glue crates that test integrated behavior. The client's binding of `client-files` and the match to Bevy rendering is such glue, and it can be tested today only from inside the binary.
- *Direction:* a `client` library (the glue) with a thin binary. Worth doing once batch "Campfire on the stack" ports the client onto the stack.

**A20. `import` holds three parts.** Confidence: medium for the facts, low for the direction.
- *Evidence:*
  - `import` is 21,850 lines, the largest crate. Its parts:
    - game-agnostic glTF and KTX2/DDS/TGA writers (`gltf/` 674 lines, `texture/` 932 lines);
    - the Zero Hour format readers (`zero_hour/format/` 9,118 lines: BIG, CSF, INI, map, ref pack, W3D);
    - the conversion into packages.
  - The readers are engine-free except for `campfire_mode::OptionData` (`format/ini/multiplayer_ini.rs:1`) and `campfire_client_files::ModelScale` (`format/ini/object_ini/mod.rs:3`).
  - `02-engine-core.md:101` ("Game-bound code") keeps game code in the module `import::zero_hour`.
- *Why it matters:* the readers are problem crates in shape. Under the current design, though, the split would be one crate for one game.
- *Direction:* no change now. Once a second importer or the planned `editor` needs the generic writers, move `gltf/` and `texture/` into a problem crate that writes the generic formats.

### Where these crates meet the network stack

- **A21. The stack depends only on engine-free base crates.** `net-bytes` and `net-replication` depend on `codec`; `net-connection`, `net-messages` and `net-udp` on `common`; `net-endpoint` and `net-packets` on `store`; `net-port-map` on both `store` and `common`. Plan batch "The stack on Campfire's base crates" adds `TickRate` and `ChannelBinding` to `common` and `SplitMix64` to `math`. This agrees with A1: those base crates should be marked as problem crates before more of the stack leans on them.
- **A22. The session host lives in `net`.** `net` requires 21 Campfire crates (bots, capabilities, mode, package, runner, protocol and others), while `server` is 958 lines. After batch "Campfire on the stack", the session host is the glue between the session crates and `net-stack`. That is the network audit's domain.
- **A23. `state-types` and `visibility` are the replication vocabulary that `net-replication`'s audiences will read.** A4 would let the stack name them without the sim.
- **A24. Five crates name `lightyear` 0.30 directly** until batch "Campfire on the stack": client (`bot.rs:9`, `view/mod.rs:32`, `main.rs:39`), server (`main.rs:38-39`), harness, lan-check (through net) and net.
- **A25. The only duplicated function bodies are transitional.** Across the workspace, identical bodies of six or more lines occur only between `quic` and `net-udp`/`net-port-map`, which `quic`'s deletion in batch "Campfire on the stack" removes. None occur among the crates of this audit.

## Checked and found consistent

- Every crate inherits the workspace lints.
- No crate re-exports another crate but `common` (its derives).
- No OS `cfg` appears outside `store`'s `platform` module.
- `rhai` is named by `script` alone.
- `serde_json`, `toml` and `postcard` are named by `codec` alone.
- Every crate with an `internals` feature forwards it to each normal dependency that has one, and lists itself as a dev-dependency.
- Every bench crate follows the pattern: a gated `pub mod bench`, `bench.rs` beside the code, `[[bench]] required-features = ["internals"]`, `criterion` optional under `internals`. The exception is `harness`, which is all test code and keeps two bench targets, one of them for its global allocator.
- The crates whose `internals` gates no code of their own (interaction, items, state-types, visibility, verifier, import) only forward the feature, which "every test build turns `internals` on" requires.
