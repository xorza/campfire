# Workspace alignment

The workspace reads as one engine. Each crate has one kind and one job, and depends only on what that job needs. The network stack follows Campfire's rules as Campfire's own crates do. Each shared tool has one home, each word one meaning, and integrated behaviour one way to be tested. The proposal builds on [Engine core](02-engine-core.md), [Lightyear in the tree](12-lightyear-in-tree.md) and [The network stack](13-net-stack/layering.md). Its facts are in [Workspace audit](../research/workspace-audit.md), whose finding IDs are unique across its five files; the audit's coverage table gives each finding's decision here, or the reason it is not taken.

## Causes

Six causes explain nearly every finding. Each section of the decisions removes one of them.

- **The kinds of crate are a rule of the stack alone.** `13-net-stack/layering.md` defines problem crates, glue crates and test support, and `rules` checks a hand list of 8 stack crates, which missed `net-port-map`. Campfire's crates have layers but no kinds, so `values` and `visibility` carry the ECS for a few derives, the harness carries the network for one of its parts, and `store` parses Rust. The checker itself has a hole: it reads comments as code (A1 to A4, A7 to A12, B2, B14; E40).
- **Batch 1 held the stack to what a test checks, not to what review holds.** The lints and `rules` passed, and the conventions that no test holds stayed the fork's: module paths as API, plugins that install their own dependencies, free-function systems, unreachable states, doc examples and few citations (B15 to B20).
- **The stack brought its own tools.** Its bench policy, allocation counter, log capture, OS randomness, hasher and seeded draws sit beside Campfire's, and two of them break decisions of 02 and 05 (C4, C5, C9 to C12; E29, E31).
- **The stack's crate boundaries follow the fork's history.** A grab-bag crate, a serde bridge in the header codec, registry hashing in the vocabulary crate, a clock edge in the link crate, and features with no caller put dependencies where no code needs them (B1, B3 to B6, B9).
- **Names has no owner across the boundary.** The renames of batch 1 stopped at the type, the stack's words collide with Campfire's, and both sides break the suffix rules in their own ways (D1 to D31; E1, E3, E32, E38).
- **Two ways to test one thing.** The harness and the stepper test integrated behaviour with different clocks, link models, log checks, schedules and homes (B21, B23; C9, C10, C19).

## Decisions

### Order

- **The batches go by impact.** A batch that fixes what is wrong now, or makes every later batch safer or cheaper, goes first; a batch never goes before one it needs. The checks lead, as a checker with a hole lets each later batch drift. The stack is reshaped and renamed before Campfire names its types, as each move after that changes Campfire's crates too. Campfire's own moves and renames, which no batch of the slice needs, follow the slice. Refused: the order of the causes, which puts names before the work that the names depend on.

### Kinds and checks

- **Every crate declares its kind in its manifest.** `[package.metadata.campfire] kind` is `problem`, `io`, `proxy`, `glue`, `test`, `check` or `app`, and `rules` reads it. A problem or IO crate has no engine crate in its required closure, a glue crate requires no IO crate, a test crate is never a required dependency of a library, a proxy is exactly a proxy of the bans, and the crates table gives each crate's kind. An IO crate holds a network IO that reaches the OS; the emulator's wires, which run in process, leave `net-packets` a problem crate. Refused: the hand list in `rules`, which covered 8 of the 16 engine-free crates; the crates table as the source, prose that no test parses (A1, B14).
- **The kinds are an engine-wide decision of 02.** `layering.md`'s kinds, and "A plain type the ECS holds takes its ECS derives behind its crate's features", move into 02's Structural rules for every crate. Refused: a rule of the stack alone, which left `values`, `visibility` and `client-files` on the ECS for six files (A1, A2).
- **`bevy_platform` and `bevy_math` are engine-free.** A problem crate may hold them, as they hold no ECS and no app. Refused: Bevy's math counted as engine, which keeps `client-files` glue for its vectors (A3).
- **`rules` reads Rust as Rust.** `rules` parses sources with `syn`, as `checks/vocabulary` does, and reads manifests through `codec`'s TOML; `SourceFiles` and `CargoManifest` leave `store`, which holds files, threads and the platform alone. Refused: a fix to the line scanner, which misreads raw strings and takes comments for code (A10, B2; E40).
- **A structural rule lives in `rules`.** `capabilities`' scans of the capability crates' sources and of `bots`' become tests of `rules`, which reads every crate (A11). The tests of `bots` and `runner` that load the test packages stay beside the private items they read: what 02's test homes protect is that no test build holds a second copy of a crate, which a dev-dependency on test support above a crate does not break, and `rules` already holds that (A12). Refused: moving those tests to `harness`, which widens their modules' visibility for a test.
- **A crate depends only on what its code names.** A required dependency is one that production code names, a test dependency is a dev-dependency, and one that only `internals` names is optional under it; `harness`, `navigation`, `bots` and `net-transport` follow (A8, A14, A15, B2).

### Campfire's crates

- **`values`, `visibility` and `client-files` are problem crates.** `values`' four components keep their ECS derives and impls behind its `ecs` feature, and its two holders of `StableId` take the id's plain form. `visibility` holds `DataKind` and `Recipients`, which `state-types` names from it, and `OutView`'s `Resource` derive goes behind `ecs` (A2 to A4).
- **A rule lives with its owner.** A slot's charges move from `progression`'s `Learning` to `actions`, which owns slots (E26).
- **`common` holds plain shared values alone.** Its allocation audit and `backtrace` move to the measurement crate, and 02's list of its dependencies names `zeroize` (A9, A17).
- **A directory names a domain, and the manifest names the kind.** `geometry` and `schedule` move to `sim/`, beside `math` and `sim`, and `verifier` to `apps/`, beside the other shipped binaries. Refused: a directory for each kind, which splits the stack across five directories (A18).
- **One rule in one place.** An area's targets and an aura's select through one filter, and a splash reaches by the one reach rule, `≤`. Refused: two filters and two reaches, which already disagree on neutral and dead units and on the boundary (E24, E27).

### The stack held to Campfire's conventions

- **The stack stays on Bevy's plugins.** `Module` is one entry of a match's list, with a capability, books and the sim's stages, which a network layer has none of; the stack's layers are plugins in groups, as `net` is. Refused: the stack as `Module`s (B20).
- **A plugin group lists every layer.** The stack's groups and Campfire's apps add each plugin they need, and no plugin adds another under `is_plugin_added`, as Bevy's own groups do. Refused: self-installation, which hides the order of plugins and gives one plugin two installs (B20).
- **One path for each item.** A crate root has no `pub mod` but `internals` and `bench`, which `pub use` each item; `internals/` directories hold fixture trees alone, and a gated helper sits at the end of the file whose privates it reads. This is also `13-net-stack/api.md`'s rule, which the stack breaks 37 times (B15, B18).
- **The stack's systems are methods.** Each system takes its plugin or its component as its home, as Campfire's do, in one pass. Refused: a move as each file next changes, which never ends (B16).
- **A state the code cannot reach is a type.** A reliable channel's send state is an enum of its states, with no `unreachable!` arm; the server's side of the handshake has no `connect`; a port map's error names its cases, not a `String` (B19).
- **A header decodes from one encoding.** A `ToBytes` decode refuses an `Option` tag other than 0 or 1 and a varint longer than its value needs, as 02 decides for every format. Refused: two spellings of one header, which a third party's implementation of 05 would meet (C18).
- **The stepper runs Campfire's strict schedules.** Each stepper app makes `Update` and `FixedUpdate` strict through `schedule`, so an ambiguity fails the layer's own test, not the batch that moves Campfire onto the stack (B21).
- **The stack's documentation is Campfire's.** Code holds one-line docs and no examples or tutorials, which no lane compiles, and each code that holds a decision of `13-net-stack/` cites it (B17).
- **The driver's lanes are 02's.** The lint without `compression_lz4` is the `ship` lint at default features; the rustdoc lane that `testing.md` plans goes, as 02 decides "No docs lane"; coverage stays planned (E33).

### One home for each tool and helper

- **One measurement crate.** `campfire-measure`, a test crate with no engine, holds the bench kinds and sampling policy, the counting allocator with its process window, the allocation audit and criterion's allocation measurement; every bench and allocation test uses it. 02's process window wins over `testing.md`'s per-thread counter, which misses a worker and asserts one task-pool thread that the workspace's Bevy, with `multi_threaded`, does not keep (A9, B24, C10, C11; E29, E31).
- **One log check.** `campfire-log` gives a `LogCheck` scoped to a closure, and the stack's two `ErrorCapture`s go, so a warning fails a stack test as it fails Campfire's (C9).
- **One source of OS random bytes.** `store`'s platform layer draws them through `getrandom`; `protocol`'s `Entropy` and the stack's `Random` read it, and `net-udp`'s second source and `ring`'s `SystemRandom` go, so a seeded run seeds every draw (B8, C5).
- **A seeded draw means one thing.** `math`'s `SplitMix64` draws `below`, `chance` and `fraction` by `Rng`'s exact rules; the stack's biased `below` and `next_f32` go (C4).
- **The registry hash is a domain hash.** The protocol registry hashes through `DomainHasher` under a `campfire/<name>/v<n>` tag, as 05 decides for every hash; `StableHasher` and `net-core`'s `blake3` go, and the stack's maps name one map type (B4, B8, C12).
- **The UDP labels keep the published key.** The TLS seed's label is `campfire/tls/v1`, `quic`'s, and the reset key's `campfire/udp-reset/v1`, so a server's key file gives the same `TlsKey` after the move to UDP. Refused: a new TLS label, which changes each listing's pin once (C8).
- **One type of the server's TLS key.** `common`'s `TlsKey` replaces `net-udp`'s `ServerKey`, as Campfire's "server key" is the session's signing key; the server secret and the token secret hold their bytes in `common`'s `Secret<32>` (C7, C8).
- **One tick rate type, two resources.** `common` holds the plain `TickRate`. The sim reads the match's rate, which only the terms set, and the timelines read their own, which `SetTickRate` sets, so a sped-up local match keeps its sim's rate. This amends 12's "One tick rate" (C1).

### The stack's boundaries

- **`net-common` goes.** `Arena` moves to `net-bytes`, whose users all reach it; `ReadyBuffer` to `net-emulator`, its one user; `SplitMix64` to `math`; the mutation fuzzer to `net-bytes`' `internals`; `Random`, which `net-udp` and `net-port-map` draw from, to `net-core`, which `net-port-map` then names; the bench policy to the measurement crate. This replaces 12's "The fork's `common` stays a crate of its own", whose reason was `Arena` alone (B3, B11).
- **`net-bytes` is the header codec alone.** The serde bridge moves into `net-messages`, its one caller, so no problem crate compiles `codec`'s TOML and JSON. A `ToBytes` write goes into the stack's own sinks and cannot fail, so a message's serialize returns no `Result` (B1; E8).
- **No feature without a caller.** `TransformDiff`, `TransformDiff2d` and `transform_lerp` go, with replication's `bevy_transform` and `bevy_math`, and `net-link` drops `net-timeline`, as the plugin groups add the clock (B5, B6).
- **The emulated wires are the packet session's IO.** `net-packets` holds the emulator's IO, and `net-udp`'s emulated server and client are production code, as the sealed singleplayer link runs them; `layering.md` says both (B12).
- **The singleplayer link is sealed.** The local link runs the UDP handshake over the emulator, so the connect answer signs a real channel binding on every link. Refused: a plain link with a fixed binding, a stand-in that only singleplayer signs (C6).
- **The stack's host-client path goes.** No Campfire mode runs a client inside its server's app, as singleplayer is a local server over a link (08-sessions.md), so the path and its tests go once Campfire runs on the stack. Refused: the path as a stack feature, about 30 files that no code calls (C22).
- **Planned: the split of interpolation.** A crate of lerp rules and frame interpolation below `net-interpolation` and `net-prediction`, once a read of history ownership shows the seam (B9).
- **Planned: the split of `net`.** Once Campfire runs on the stack, its session host and its glue to the stack are weighed as two crates (B13, A22).

### One vocabulary

- **Names is the one table of words.** 02's Names gains the stack's words, the packet session, a link, a peer and its local and remote ends, a side, a body and a type id, and records `*File`, a whole file of one format, and `*Ext`, Rust's extension trait, which a call extension is not. Each rename follows it to the last field, file, document and test (D24, D30).
- **The stack's words that break Names change.** The packet session is `PacketSession*` everywhere. A Bevy hook is named by what it does, and its six identical pairs are one. `*Kind(TypeId)` is `*TypeId`, a message's `*Data` is `*Body`, `*Context` is `*Cx`, `Owners` is `Holders`, and `MemberSpec` and `VisualError` drop the suffixes they misuse. Lengths are `*_len`, counts are plurals, bounds are `max` and `min`, a predicate is `is_*`, a span is begun and ended, and `TickDelta` is `TickSpan` (D1, D2, D4, D5, D9, D15 to D22).
- **Settings are a `*Setup`.** Every stack `*Config` is a `*Setup`, as Names gives `*Setup` to what a thing is built from. Refused: `*Config` for what a running system reads, a second word for one meaning (D7).
- **A type name that both sides declare keeps one meaning.** The narrower use takes a qualified name, as `ByteArena`, `StepOrder` and `UnixClock`; `store`'s `Slot` and `import`'s `MapFile` and `UnitsFile` follow (D10, D30, D31; C23).
- **Nothing names the fork.** The WebAssembly remarks, the `Raw*` connections, and the documents' "simulated", `SimWires` and `CrossbeamIo` for the emulator go; the `lightyear_*` bans go with Campfire's Lightyear (D29).
- **A test name is a sentence.** The stack's 318 short test names are written as Campfire's are (D23).
- **A Bevy plugin is a `*Plugin`.** Every plugin is a `*Plugin` and every group `*Plugins`, as Bevy and the stack name theirs, so a bare name is a Campfire `Module`. Refused: bare plugin names, which give the client's `Orders` plugin the name of the `Orders` module (D13).
- **A system set is `*Systems`.** Every system set ends in `*Systems`, as Bevy 0.17 and the stack name theirs, and `*Set` names a collection alone. Refused: `*Set`, which also names 15 collections (D14).
- **Script state's receivers are its audience.** `StateSync` and its kin become `StateAudience` and its kin, and the scripts' key `sync` becomes `audience`, as `10-replication-audiences.md` names it; `sync` stays the timelines' sync and `store`'s fsync. Refused: one word for three meanings (D3).
- **A localized text is a text.** `MessageId` and `NamedMessage` become `TextId` and `NamedText`, as the network and Bevy mean a sent value by "message" (D6).
- **A field of a tick rate is `tick_rate`,** and a module gated by `test` is not named `internals` (D19, A16).
- **`rules` holds the suffixes.** A test reads the declared names and refuses a system set not named `*Systems`, a plugin not named `*Plugin`, a `*Manager`, a `*_hook` function outside the scripts, and a `*Kind` that wraps a `TypeId`. Refused: review alone, which let both sides drift.

### One way to test

- **One rule of test homes.** A test lives in the lowest crate whose dependencies hold everything it uses, and no crate dev-depends on a crate above it; 02's "Tests live at the top" is this rule for a whole match, and `testing.md`'s for a layer of the stack. Refused: two rules in two documents (B23).
- **The harness splits at the network.** The session harness, fixed sessions, goldens and the proving and scripted matches, has no network; the network harness glues it to the stack's stepper and emulator. So `zero-hour-check` builds without the network (A7).
- **Planned: one driver of integrated tests.** The network harness runs on the stack's stepper, as 12 plans, so one clock, one link model, one log check and one schedule serve every integrated test (B23, C19).

## Open decisions

The issue log's three Decide items touch the same words and layout. The batch of Campfire's words takes each once it is settled.

- **The order's `Action`.** The order's sum becomes `Directive`, `Instruction` or `Task`, or both keep `Action` (E1).
- **The test packages' layout.** Every package by what it is, or every package by its game first (E2).
- **Crate names that are not nouns.** `declare` and `state-types` become `declarations` and `states`, or both stay; `net-despawn`, a verb, follows the same choice (E3).

## Checks

- Each crate declares its kind, `rules` holds each kind's rule, and `values`, `visibility`, `client-files`, `geometry` and the stack's problem and IO crates pass as problem or IO crates.
- A required dependency that only a comment names fails `rules`.
- `rules` refuses each suffix that Names gives another meaning, and no crate root has a `pub mod` but `internals` and `bench`.
- No production code of the stack calls `is_plugin_added` or `unreachable!` on a state.
- The stack's tests pass on strict schedules and under `LogCheck`, and its allocation audits run on `campfire-measure` with the workspace's `multi_threaded` Bevy.
- A fixed server secret gives the `TlsKey` that `quic` gives it today.
- A header with a second encoding of a value fails to decode.
- `cargo tree -e normal` counts fewer crates under `zero-hour-check`, `client-files` and `net-emulator` than at `68ca71cbc`.
- `cargo drive gate` passes at the end of each batch.
