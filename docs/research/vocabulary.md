# Vocabulary

How the repository names its directories, files, types, functions and fields, read 2026-10-19 from `cargo drive run vocabulary` at `d06d221de` with the vocabulary tool beside it: 445 directories, 2,061 files, 1,792 Rust files; 3,753 types, 9,845 functions, 6,139 fields, 2,347 variants, 1,975 values; 2,933 words. Counts leave out tests and benches unless they say so; a name of a standard trait's method, as `eq` or `serialize`, counts as a name, as the index holds it.

## Layout

- **One group is a crate too.** `crates/capabilities/` holds the crate `campfire-capabilities` (`Cargo.toml`, `src/`, `tests/`, `benches/`), 14 capability crates, and `core/`, a group of 9 crates, the one group inside a group. Each other group, `apps`, `base`, `network`, `session`, `sim` and `test`, holds crates alone. The checks are a group outside `crates/`, `source/checks/`.
- **A crate repeats its group.** `sim/sim`, `network/net`, `test/test-packages`; of the checks, `lan-check` and `zero-hour-check` end in `-check` and `vocabulary` does not.
- **Crate names mix forms.** Capabilities: `abilities`, `areas`, `items`, `orders`, `projectiles`, `stats` are plural; `combat`, `interaction`, `mode`, `navigation`, `production`, `progression`, `vision` are mass nouns. Core: `actions`, `deliveries`, `players`, `units`, `values` are plural; `geometry`, `script`, `visibility` singular; `declare` is a verb; `state-types` the one name with `-types`. The top crate's module `order` holds `Order`, beside the crate `orders`.
- **The command line has two homes.** `client` and `server` keep a clap `CommandLine` and the `Args` it resolves to in `args/mod.rs`; `import` and `verifier` keep `Args` alone in `args/mod.rs`; `zero-hour-check` and `vocabulary` keep `CommandLine` in `command_line.rs`; `lan-check` keeps it in `mode/mod.rs`.
- **Test packages go two ways.** `packages/test/heroes/<hero>` and `packages/test/modes/<mode>` group by what a package is; `packages/test/moba/{heroes,modes,spells,locales}` by the game first. Locale directories are `locale/` in every package and `locales/de/locale/hero-<name>/` in the German locale package, whose `hero-cinder` is the one kebab-case name among the packages' snake-case paths (`two_lanes`, `chain_fire.rhai`).
- **Design documents number with gaps.** `01`, `02`, `03`, `04` (a directory with `00-overview.md`), `05`, `08` twice, `09`, `10`, `12`, `14`, `15`, `22`; `docs/research/` has no numbers.
- **51 files hold no type of their name.** Most hold one type of another name, as `state_registry/builder.rs` holds `StateRegistryBuilder`, `supply.rs` `SupplySums` and `SupplyParts`, `rules_package.rs` `ModeImport`, `ref_pack/mod.rs` `Reader`, `map_dict.rs` `DictValue`, `action_rules.rs` `ActionAt` and `ListRules`, `resource_audit.rs` `ScratchCheck`, `publish.rs` `Shape`, `SegmentShape` and `Cursor`; the `capabilities/src/fixtures/<capability>.rs` files hold `<Capability>Fixtures`; 12 files beside a `Value32` alias hold its `*Meaning` alone.

## Type suffixes

The doc comments give the four suffixes of `AGENTS.md`'s one-meaning list these meanings: a `*Data` is a section as a package writes it (106 types); a `*Spec` is a section with its names resolved, as a match runs it (28); a `*Book` holds a module's specs by id (33); a `*File` is a whole file of a package, as `UnitsFile` is `data/units.toml` (31).

- **`*Spec` of no section:** `MatchSpec`, what a match is built of; `ArenaSpec`, a test arena's; the traits `HookSpec`, `ListedSpec` and the struct `MemberSpec` of the script API.
- **`*File` of no package file:** `store`'s `DurableFile`, `InputFile`, `OutputFile`, `SecretFile`, `PinnedFile`, how a file is written or read; `LogFile`, `KeyFile`, `SlotBotFile`, a file a process keeps on disk; `import`'s `DdsFile`, `TgaFile`, `W3dFile` and `MapFile`, a file of another game's format; `RepoFile`, a file git lists.
- **Errors take seven suffixes.** `*Error` 146; `*Refusal` 15, why a capability refused a script call, or a peer or a gateway a request; `*Refused` 10, net events that a refusal happened, and `ServerInputRefused`, an error; `*Problem` 8, what is wrong with a package's data or a script; `*Failure` 5, a check's failures and a worker's; `Flaw` 3, test structs; `Fault` 1.
- **A capability's data error takes three prefixes.** The crate's name: `CombatDataError`, `ItemsDataError`, `ModeDataError`, `NavigationDataError`, `ProductionDataError`, `VisionDataError`; a singular of it: `ProjectileDataError` in `projectiles`; another concept: `ModifiersDataError` in `stats`. Every `*Refusal` takes the crate's name: `AbilitiesRefusal`, `ProjectilesRefusal`, `StatsRefusal`.
- **Settings:** `*Setup` 8, `*Params` 7, `*Args` 7, `MarkerInfo` 1; no `*Config`, `*Settings` or `*Options`.

## One word, several meanings

- **`owner`:** the player slot that owns a unit (`Owner`, `owner: Option<PlayerSlot>` in 9 fields, `OwnerChanged`); the module that owns a declaration (`OWNER` in 10 crates, `owner: Capability`, `ApiOwner`, `ParamOwner`); a place in data (`owner: BookPlace`, `owner: ListPlace`); a file only its user opens (`OwnerOnly`). `AGENTS.md` lists `owner` among the words of one meaning; `02-engine-core.md` says "Books built by their owners", a module, and `10-replication-audiences.md` "the owner, the client of the slot that owns it".
- **`kind`:** the marker of an entity's kind (`EntityKind`, `KindMarker`, `KIND` in 21 crates, `OwnedOfKind`), as `AGENTS.md` defines it; and a closed category, in 35 types (`DamageKind`, `SlotKind`, `ActionKind`, `FileKind`, `EntryKind`, `DrawKind`).
- **`part`:** a component an entity carries (`Part`, `PartWriter`, `part_mut`, `PartHash`); a part of a script call (`CallPart`, `CallParts`, `CtxParts`); a system's query bundle (`Parts` in 9 crates); a model's mesh (`ModelParts`, `import`'s `Part`); a piece of geometry (`CellPart`, `body_box`'s `Part`).
- **`place`:** where in a package's data a value stands, for its error (`Place`, `BookPlace`, `ListPlace`, `TypePlace`, `ParamPlace`); a location in the world (`production`'s `Place`, `BuildPlace`, `spawn_place`, `drop_off_place`, `MarkerPlace`); a position in an encoding (`InputPlace`, `EncodedPlace`).
- **`slot`:** a player's seat (`PlayerSlot`, 62 fields `slot: PlayerSlot`); a unit's action or item slot (`ActionSlot`, `SlotKind`, 22 fields `slot: u8`); `store`'s `latest_writer::Slot`.
- **`source`:** the unit an effect comes from (24 fields `source: StableId` or `Option<StableId>`); where a book, a package or a script comes from (`BookSource`, `PackageSource`, `ScriptSource`).
- **`hook`:** a script's hook; and `SpawnHook`, `fn(&mut World, Entity)`, an engine callback of `units`, where `AGENTS.md` keeps `Hook` for the scripts.
- **`at`:** a field `at` holds an index (9), a place (13) or a tick (3).
- **`data`:** a field `data` holds a data directory (`PathBuf` 6, `Option<PathBuf>` 3) or a package's data (`&ModeData`, `&ActionData`).

## One meaning, several words

- **To begin a tick:** `begin_tick` in `sim_rng`, `units`, `route_planner`, `fog`, `sight_maps` and `sight_cache`; `start_tick`, the system that calls `SimRng::begin_tick`.
- **Lifecycles:** `ActionProgress` has `start` and `begin` with two jobs, then `start_attack`, `stop`, `finish` and `finish_cast`; `Checkpoints` has `start`, `begin` and `finish`. Pairs: `start`/`end` (`SimTick`, `TickClock`, `Segment`), `begin`/`end` (`PassQueue`), `begin`/`finish` (`SourceReads`, `Checkpoints`), `start`/`stop`/`end` (`ProcessGroup`), `start`/`stop`/`close` (`QuicEndpoint`), `start`/`close` (the writers of `store`). Function names: `start` 40, `begin` 34, `end` 26, `finish` 10, `stop` 9, `close` 8.
- **Constructors:** `new` 308, `of` 190, `build` 66, `open` 21, `create` 8, `from_parts` 1, `make` 1.
- **Context:** `Ctx` (the script's `ctx`, `ArgCtx`, `ModeCtx`, 26 names) and `Cx` (`BookCx`, `InstallCx`, `EffectResolveCx`, `EffectRuleCx`, `WindowCx`, 9 names).
- **Length and count:** `len` in 112 names and `length` in 16 (`byte_length`, `checked_length` beside `file_len`); a byte count is `bytes: u64`, `len: u64` or `file_len: u64`. Of 661 integer fields, 156 name a count by the plural of what it counts (`dozers: usize`, `columns: u32`), 14 `count` and 21 `*_len`.
- **Others:** `dir` in 57 names, `directory` in 6 (`Directory`, `RowDirectory`, `SyncDirectory`, `is_directory`, `read_directory`), `folder` in 2; `pos` in 46 (`pos: Vec3`), `position` in 12; `life` in 45, `health` in 2 (`Health`, `health`).

## Predicates

732 functions return `bool`. By first word: `check` 115, 108 of them impls of a trait method `check(&self, &World[, Entity])`; `is` 77; `holds` 44; `has` 27; `takes` 17; `contains` 17; `begin` 17, whether to go on. 279 others open with no such word, among them adjectives and participles beside the `is_` ones: `empty`, `full`, `busy`, `stale`, `hidden`, `done`, `pending`, `playing`, `running`, `homing`, `public`, `pure`, `portable`. The script API decides `is_`, `has_` and `can_` (06-script-api.md, One vocabulary); the Rust code has no decision of its own. No function opens with `get_` but `get_mut` and `get_or_default`.

## Names declared twice

113 type names are declared in two crates or more, at any visibility. Among them, types of different meanings at once in one call graph: `Action` (the top crate's sum of every action, and `actions`' action); `InputFile` (`store`'s read, `client-files`' input map file); `MapFile` and `UnitsFile` (`import`'s and `package`'s); `UnitTypes`, `Units`, `Orders`, `View`, `Mode`, `Place`, `Part`, `Slot`, `Scratch`, `Stats`, `Team`, `TickClock`; and `Args` 4 times, `CommandLine` 5 times.
