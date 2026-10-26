# Review: whole workspace

Review of `source/` (about 470,000 lines in 79 crates, the checks and the packages) on 26 October 2026, by eight reviewers in parallel, each over one part, with the bugs spot-checked against the code. Paths are relative to `source/`. Line numbers point to the code at `594b9952c`.

The test suite: 2,460 tests pass in 23 s of wall time (102 s of CPU). Run one at a time, no test passes 1 s. The tests nearest the bound are `scenario::a_server_stall_runs_at_most_its_bound_and_its_held_orders_apply` (0.96 s), `session_logs::a_3v3_log_with_learn_orders_verifies_from_the_store` (0.94 s) and `local_server::tests::a_client_plays_a_local_server_which_publishes_the_log_as_it_drops` (0.80 s).

What holds up: the module and declaration machinery, the deterministic math and geometry (every rounding derived, ties exact), the hot passes of navigation, stats, combat and the transport (no allocation once warm, ordered by stable id, no hash iteration, no float), the handshake, admission and port map against their RFCs, and the split of the transport into its crates.

## Proposal

The findings come from nine causes. The target design removes each.

1. **A fact with two owners goes out of step.**
   - Work in progress copies its action and rank from its slot, but the readers and the restore check go back to the live slot. So a learn during a channel makes state that the restore refuses, and a refilled slot starts the new action at the old target.
   - A unit's despawn has two owners: `units` ends a lifespan with no regard for `Dead` or `Kept`, and combat despawns the dead.
   - In prediction, three lists of local-timeline ticks do not move on `LocalTimelineShift`. The "spawned at T, despawned by a rollback past T" list is written twice, once in replication, which never rolls back.
   - `PredictionCounts` is written by `RollbackPlugin` and created by the diagnostics plugin.
   - Target: `InProgress` is the one record of what runs, and one `ActionProgress` method means "the slot changed under it". The dead are combat's alone. One prediction-owned `RollbackSpawns` holds every local-tick list and moves with the timeline. Each plugin creates what it writes.
2. **A peer-fed or data-fed structure is bounded in bytes, not in work or count.**
   - The reliable channels' gap store inserts in the middle of a sorted `VecDeque`, and it admits ids that no honest sender sends.
   - Vision's reveal counts are `u16`, a script can add reveals with no bound, and a map can size vision's maps at about 1 GiB.
   - `InputFile::read_at` allocates before it checks the range, and the BIG reader passes it an untrusted header length.
   - A closing session ends on a Close header that it did not authenticate. Two constant-time compares are hand-rolled with no `black_box`. The protocol check travels inside the registries it checks.
   - Target: every peer-fed structure costs O(log n) per item, and every id outside the sender's own invariant is a protocol violation. Every count a script or map can raise has a load budget or an `ApiError`. Every check of agreement or authenticity runs before data flows, with the shared `Secret` type.
3. **A fallible read that cannot report its failure.**
   - The journal walk of the publication stores a read error where `SessionLog::publish` cannot see it. The log is renamed into place truncated, and the next start takes the session as published.
   - The restore reads the whole journal (up to 4 GiB) and collects a `Vec<&[u8]>` of its records, while the publication streams the same journal.
   - Target: one streaming, fallible journal reader in host that feeds both the publication and a `JournalReplay` builder for the restore.
4. **Whole work where one item, or once, does.**
   - The import's self-check runs a full `ModePackages::new` for each of 150 maps, and the Zero Hour check runs one for each of its 8 plays.
   - The load builds its name tables in three `BookCx`, and seven `Declarations` consumers each walk the whole release for one projection.
   - Each static spawn at a match start rebuilds the whole static index, O(S²) for up to 2,683 objects.
   - Each tick, vision recounts every live reveal, `LiveCarriers::forget` scans every carried change 10 times, and two grant passes make the same scan.
   - A bot's param converts on each read, and the rules test recomputes each file's gate, crate and blanked views per rule.
   - Target: `ModeRead` keeps the map-free part, and `on(map)` adds only the map. One `BookBuild` holds one cx. One `DeclarationCatalog` is built for each capability set. Each index has a path for one item. Reveals are counted at arrival and at expiry. Each pass returns at once when nothing changed. A call extension holds values converted once. The rules' `Workspace` indexes each file once.
5. **The life of a session on disk has no home above protocol.**
   - The client and the verifier each read and decode a log file, read snapshots, and choose between a start at tick 0 and a resume.
   - A report's release is never checked, and a report travels as a save (`SaveCommand::Report`, `SaveRefusal`).
   - The seed switch, the checkpoint record and the snapshot fingerprint are each written twice. `record_*` means "check" in one place and "sign" in another.
   - `SessionLog` keeps the file length by hand in eight places.
   - Target: host owns the log on disk (`ReportDir::read_log`, `SnapshotDir` as a `SnapshotSource`, one log read with the release check). `Session` owns its lifecycle (`open(.., Option<ResumeAt>)`, `draw_from`, `last_seed`, `CheckpointBegun::record`). The words are `JournalCommand`/`JournalRefusal` and `serve_*`. A `FileLen` type sits beside the encoder in `file.rs`.
6. **A shared shape is kept by hand in several copies.**
   - Sorted unique `Vec` states appear nine times. The standing-order loop on a due index appears five times.
   - The registry plumbing appears three times, and RFC 9000 §A.3 decoding twice.
   - The creep and tower AIs are copied across three modes and have drifted. `Golden` and `Measured` are twins of 19 fields, and the import's loss messages are written twice.
   - The ban lists in the rules test copy `clippy.toml` and have drifted by 15 entries. Some client files check their rule in a constructor, others only in `Deserialize`.
   - Target: `SortedSet`/`SortedMap` in `common`, a standing-order helper on `DueIndex` in `sim`, shared `DeferredInstalls` and one `matches_filter` in net, and one §A.3 `const fn`. One creep and one tower AI live in a rules package. Each pin has one field. `ImportLoss: Display`. `clippy.toml` is the one list of bans. Every `ClientFile` is built through its check and written through `encode`.
7. **Generality with no second use.**
   - `Timeline<T: TimelineSetup>` and `NetworkTimeline` carry nothing generic, and the `TimelineSync` trait reaches duplicated fields through six accessors.
   - `SerializeFns` keeps five layers of function pointers for custom encodings that only one test uses.
   - Other examples: `wrapping_id!` (one use), the `Writer` and `ByteArena` methods that only tests call, and the type aliases that only rename.
   - Target: plain structs, one `SyncController` value, two monomorphic encode functions (mapped or not), and no API without a production caller.
8. **Test support works around engine checks, or rebuilds a fixture for each case.**
   - `ScriptFailures` has no `Drop` check, so `Orders::think` asserts by hand, and failures come back in three shapes.
   - Each package flaw reads the whole MOBA tree from disk again (about 380 walks and 30,000 reads), one flaw at a time.
   - The harness takes a player slot as `u32` in one method and as `PlayerSlot` in the next. `AppendWriter` reads the wall clock itself, so its tests sleep 450 ms.
   - Target: engine checks fire at drop, fixtures are read once and cloned for each case, sweeps run on `threads::Parallel`, harness entry points take newtypes, and clocks come in through the bounds.
9. **A fact shared by capabilities lives in the first capability that reads it.**
   - Projectiles depends on stats only for `MoveSpeedCap`, and mode depends on production only to order after `ProductionSystems::Finish`.
   - The import's model stage depends on its map stage only for `condition_names`.
   - Target: such facts and schedule contracts live in `units` or `sim`, below every capability.

### Decisions taken

The user took each decision on 26 October 2026:

- The net surface with no app caller stays (11-net-stack/api.md, The stack keeps its general features that no app calls yet), so batches 4 and 12 keep all their items.
- The protocol check travels in the ALPN (11-net-stack/transport.md, The hashes travel in the ALPN, planned): batch 3.
- The IO thread sends the frame's datagrams (11-net-stack/packet-session.md, The IO thread sends, planned): batch 13.
- `client/units.toml` stays TOML (02-engine-core.md, The import's client units stay TOML).
- `SpawnParts` holds a `SyncCell`: batch 19.
- Each book builder moves to its type's file: batch 20.
- A module's systems and script API functions stay associated functions of a unit type of the module, and a platform's or proxy's entity may be a unit type (AGENTS.md, Each operation has one home).

## Plan

### 3. Peer-fed work and authenticity (net-channels, net-packets, net-udp, net-stack)

Files: `network/net-channels/src/{gap_store.rs, channel_receiver/mod.rs}`, `network/net-packets/src/{packet_session/mod.rs, stateless_reset/mod.rs, new_reno.rs, sent_packets/mod.rs, endpoint_core/mod.rs, establish.rs}`, `network/net-udp/src/address_token.rs`, `network/net-stack/src/protocol_check_plugin/`.

- [ ] `protocol_check_plugin/mod.rs:44,80` sends the check as a registered event, whose ids shift in a mismatched build, and sends it on `Add<Connected>` together with data, against transport.md "Both peers agree on the protocol before data flows". → The ALPN names the app and the hash of the three registries, alert 120 maps to `ProtocolMismatch` in `net-udp/src/alert/mod.rs`, and `ProtocolCheckPlugin` and its event go (11-net-stack/transport.md, The hashes travel in the ALPN, planned). Add a wire test with two different registries. **Question:** see `review-workspace_QUESTIONS.md`, Batch 3.

### 15. Typed data between the import and the scripts (mode, import, zero-hour package)

Files: `capabilities/mode/src/mode_data.rs`, `apps/import/src/zero_hour/{map_import/mod.rs, rules_package/mod.rs}`, `packages/zero-hour/scripts/{mode.rhai, bot.rhai}`.

- [ ] `rules_package/mod.rs:204-221` puts each faction's data into param names (`start_building_<faction>` and seven more), which the scripts join at run time (`bot.rhai:110,122,128,146,155,178,212`, `mode.rhai:36-51`), so a misspelt name fails at run time and not at the load. `ModeParam` has no table (`mode_data.rs:205`). → `ModeParam::Table`, and `factions.<faction>.{…}` that the load checks. **Question:** see `review-workspace_QUESTIONS.md`, Batch 15.

### 19. Names, docs and small code (actions, abilities, script, units, declarations, interaction, navigation, geometry, math, sim)

Files: about 70 sites of a bare `slot: u8` (`ActionSlots`, `ActionProgress::order`, `SlotsMut::fill`, `AbilitiesEffect::Learn`), `capabilities/core/declarations/{Cargo.toml, src/callback_table/mod.rs, src/spawn_parts.rs}`, `capabilities/core/script/src/scripts/pool/`, `capabilities/core/actions/src/targets.rs`, `capabilities/core/units/src/units.rs`, `capabilities/interaction/src/interaction_rules.rs`, `capabilities/navigation/src/{progress.rs, steering/mod.rs}`, `sim/geometry/src/grid_search.rs`, `sim/math/src/{wide_division/portable.rs, vec3/mod.rs}`, `session/host/src/server/server_phase.rs`, `apps/server/src/opening/mod.rs`, `base/store/src/append_writer/mod.rs`.

- [ ] "Pool" means a script budget (`scripts/pool/mod.rs:10`, `ScriptPools`, `Role::pools()`) and a stat pool (`PoolId`, `PoolCost`, `PoolPurse`, `Pools`). → The script side says "budget". **Question:** see `review-workspace_QUESTIONS.md`, Batch 19.

## Changed

- Batch 1, `ActionProgress::fits`: a channel's slot holds a rank at or past the channel's (`rank >= at`), where the plan named `book.is_ranked`; the started order's check stays as it was, as an action charges at every rank or at none (`action_data::charge_at` reads one `charging` section), so a learn cannot break it.
- Batch 1, `end_lifespans`: besides `Without<Dead>`, `Lifespan::due` reads no time while its unit is dead, and an observer of `Dead`'s removal pushes the unit again, as the due index's audit requires every unit with a time to be held.
- Batch 2, the publication's walk: it returns `Result<(), WalkStopped>`, a marker the host maps back to the read error it keeps, where the plan named `JournalStreamError`, as the walk's failures are the host's own (`InputFile::stream`'s `PathError` among them), which protocol does not name.
- Batch 2, the restore: besides `JournalReplay`, a packet record decodes its inputs as each pass reads them (`PacketInputs`), so the replay keeps no scratch of borrowed inputs across records; `InputFile::stream_within` bounds the journal and stamps it, and `read_stamped`, with no caller left, went.
- Batch 3, the gap store: a B-tree keyed by each id's bits, which finds an id and orders none, where the plan named a map by distance, as the store only ever takes the id it waits for; an entry charges `3 · (key + value) + 16` bytes, a bound of its share of the B-tree's nodes.
- Batch 4, the local-tick lists: an observer of `LocalTimelineShift` moves `local_despawn`, `local_skip_despawn` and each `PredictionDespawned` tick, where the plan named one `RollbackSpawns` for these and the pre-spawns; the pre-spawns' list stays in replication, as its registration filters by replication's own receivers and confirm history, which prediction would copy.
- Batch 4, a prediction despawn outside `FixedMain`: an entity only this end predicts goes at once, as no replay despawns it again; a server-held entity or a pre-spawn keeps its disable at the local tick, which its server or its match or expiry ends, where the plan named an assert, which the pre-spawn lifecycle and its tests refuse.
- Batch 5, the reveal counts' overflow: the counts are `u32`, so no number of reveals that fits in memory passes them, where the plan named a bound on each team's reveals with an `ApiError`; the cells-times-teams budget bounds what the wider counts cost.
- Batch 5, the allocation audit: the battle sees, on a vision grid with each fighter's sight, where the plan also named progression's experience share and items' `want_items`, which need a mode's books the battle does not load; the budget names two respawns' removals, as two units now respawn in one tick.
- Batch 8, the load once: `ModeBase` holds what a mode is on any map, its data read and laid, its unit types read, its scripts compiled, which `ModeRead` reads at its first map and each later map's `ModePackages` shares through an `Arc`; the load's checks and books still run on each map, as its books read the map. The Zero Hour check holds one `ModeRead` for its check and its plays.
- Batch 8, one book build: `Release::checked_books` runs the rules, the caller's check and the books in one `BookCx`, the caller's error apart in `CheckedBooksError`, where the plan named a `BookBuild` value, as the cx borrows givens the build makes for itself (`MoveSpeedCap`, `ModeTeams`); `build_all` skips the facts the load took.
- Batch 9, the log on disk: `ReportDir::read_log` reads and decodes a report's log for the client and the verifier, and `ReportDir::read_facts` refuses another release; the verifier's `verify_file` keeps its own read of a log file at a path, which no report holds.
- Batch 9, the test-only APIs: `run_segment_each` moved behind `internals`; `applied_inputs` stays, as the LAN check, a shipped check, calls it.
- Batch 9, the start or the resume: `Replay` checks its source in one private `source` for both constructors, and the session directory makes one `SessionSource` before it chooses; the public `Replay::of` and `Replay::from_checkpoint` and the verifier's choice stay, as their errors differ (`StartError`, `ResumeError`) and 26 callers name them.
- Batch 9, the journal commands: `JournalCommand`, `JournalRefusal`, `JournalTaken`, `JournalRefused` and `PendingJournalCommands` name them, and `JournalCommands::take` sends a save or a load to `Checkpoints::save` and `Checkpoints::load_latest` and a report to `Reports::take`.
- Batch 10, the bots' allocations: `TickBots`' doc now says what holds, that a tick of no think allocates nothing; a think's orders still allocate, once each 10 s for each bot, which buffers kept across thinks would not make measurable.
- Batch 13, the plugin that adds another: `PacketSessionLinkPlugin` is public, and the stack's group, the stepper's stack and the endpoint's tests list it once; the transport's and the messages' test stacks keep their `is_plugin_added`, as test fixtures, as batch 12 left the prediction fixture's.
- Batch 13, net-bytes' test-only API: `Writer::fill` and `Reader::take_remaining` went; `Writer::with_capacity`, `into_bytes`, `reset` and `ByteArena`'s stay, as other crates' tests and the benches build with them.
- Batch 14, `ClientUnits` and `MapLights` private: `MapLights` had its checked `new` already; the import completes its looks over several stages, so it builds them as an `ImportedLooks` of three maps and makes `ClientUnits::new` once, before the write.
- Batch 15, the slot's start and rally: the markers keep their `name`, and a start and a rally carry the tags `start` and `rally` and the param `player_slot`, from 0 as scripts count a slot; `rally` joins the spawn tags, as rallies were spawn places under `start` before.
- Batch 17, the bans' copy of `clippy.toml`: the rule keeps its own lists, as a list read from the file would pin nothing, and gained the 16 missing entries and the reverse check, so a ban of no group fails as a deleted one does.
- Batch 17, the text scans: `env::temp_dir` is a `clippy.toml` ban, and the LAN check's run root is required, as the driver always names it; the wall clock reads by `NamedPath` and the removals by `production()`; the error rule reads each `#[error]` message and its source fields by syn, and keeps its scan of production lines for the log fields and format strings, which macros hold as tokens.
- Batch 18, the shape of taking failures: `ScriptFailures` fails a test at its drop, and `take` went for `take_calls`, which every taker reads; the abilities' `failed_calls` and the mode's `failures` stay, as each capability's tests compare the shape they read. The arena's kept list forgets at its drop, as a log its test reads.
- Batch 18, `run_until(Tick)`: it keeps its `u64`, as its 134 callers name a tick by its number; the plain step loops became `steps(n)`, past those over a `Match`, a client or a bench's `i64`.
- Batch 18, the flaw tables: each loads its flaws on every core; the tree is still read for each flaw, and a flaw still names its package by a name or a directory, as the load reports either.
- Batch 18, the harness's player slots and ticks: `planned(packages, plan)` takes a `.rules()` setter; the slot parameters stay `u32` and `next_tick` stays `u64`, as about 120 callers write the numbers, and a `PlayerSlot` at each catches no case.
- Batch 18, the client's bare apps: `test_app::assets_with` serves the two asset apps of a source of their own, and the gated `impl` of `ClientLoad` moved into a module at the end of its file; the bare apps of one system or one plugin stay `App::new()`, which a `test_app::bare()` would only rename.
- Batch 18, the copies in the tests: `linear_correction!` gives the correction tests' types their curve and diff; the hero's walk is `HeroWalk` in the harness.
- Batch 19, the wide division's one rounding: a numerator of 128 bits splits into no two parts each exact in an `f64`, so the doc counts the six roundings, still within 2⁻⁵⁰, and the code stays.
- Batch 19, the search order: `GridSearch::prepare` builds it as the navigation book builds, at the load, as an install holds only systems.
- Batch 20, the book builders: 27 types' builders moved to their types' files, each load file keeps its capability's own walk, and `navigation_books.rs` holds one `impl Navigation`; the builders of `SpawnSpec`, `Weapon`, `OnDeath` and `Sight` stay in their load files, as each type's data names the type, so the move makes a cycle of modules, which the rules refuse.

## Declined

- Batch 4, a `RollbackWindow` resource: the window is the minimum of two public settings an app may change at any time, so a copy kept at finish goes stale, and the minimum costs nothing to compute at each reader.
- Batch 4, `rollback_plugin/mod.rs:424` `max(0)` as a `debug_assert!`: a rollback of a skip-despawn entity's protection starts past the current tick (`a_skip_despawn_entity_joins_rollback_after_its_spawn`), so the clamp is reached.
- Batch 7, the match start's static index: the cost is quadratic but small and once: 500, 1,000 and 2,000 static spawns took 12, 39 and 153 ms in the test profile, whose workspace code builds unoptimized, so a 2,683-object map's start costs tens of milliseconds once. A path for one body still moves the index's sorted entries and rebuilds its row index, both linear, and a spawn batch that defers navigation's callback changes when every capability's spawn callbacks run; neither is worth the one start.
- Batch 8, a `DeclarationCatalog`: two of the seven consumers, `KindsOnly` and `StateOnly`, adapt generic visitors, which a catalog cannot hold, and the other five serve callers that each need one projection at another time (the API's binding, the load, the session's start), so one catalog would build every projection at each of those calls or need an owner every caller reaches, to save their empty methods.
- Batch 8, `PackageReader::open` resolving each read: the open refuses a package missing a file before any read, so no match starts on an incomplete package, for one listing of each directory at each open, a few milliseconds at a session's start; the zip packages of `.notes/package-archive.md` change the listing anyway.
- Batch 9, a `FileLen` type: each mutator's growth of the file is the growth of the exact field it writes, beside the write, and `encode`'s debug check holds them in step; a type for it would move each formula away from its write without removing a case.
- Batch 10, one producer of the player and upgrade grants: each scan tests a few change ticks for each unit, microseconds a tick at a thousand units, and one producer would join two grant kinds whose visits differ.
- Batch 10, `LiveCarriers::forget` returning early: it prunes the changes of gone sources and carriers too, and a prune put off to a later visit leaves an entry whose entity a new unit may take; its cost is a lookup for each live change in each gap, tens of changes in the 3v3.
- Batch 10, a bot order's codec error in `NotADirective`: `ApiError` is `Copy`, so every refusal of the API carries no allocation, and the message of a bot author's typo would change the type every refusal passes through, on a path a think runs once each 10 s.
- Batch 10, the Zero Hour bot script's counts: the bot thinks within the budget the check measures, and a rewrite of its counting moves the check's measured operations for no change a player sees.
- Batch 11, `MoveSpeedCap` in units and a units set in place of `ProductionSystems::Finish`: `a_capabilitys_crate_depends_on_exactly_what_it_needs_or_uses` holds a capability's crate to depend on the capabilities it needs or uses, and the workspace refuses an unused dependency, so projectiles keeps naming stats' type and mode production's set, which the two moves would leave unnamed.
- Batch 11, `SortedSet` and `SortedMap` in common: the nine states are sets of ids, maps of ids to counts and to ticks, each checked at its restore as its own rules need, some against the world; one container would carry each state's invariant as a parameter and rewrite nine serialized forms that snapshots hold, for one binary search each writes in a few lines.
- Batch 11, `StandingOrder` in sim: production's trait already serves its two loops; the routing's, the attack-move's and the action progress's each keep a unit by their own part's test, so moving the trait below them reshapes three crates' passes for loops of about ten lines each.
- Batch 12, the `Timeline` generic, a `SyncController` and the registries' shared plumbing: each reshapes the public types of two or three crates of the general surface the user kept, with no defect behind it; left for when a change to those crates meets them.
- Batch 12, `HistoryDiffReceiver`'s double computation, `DeferredEntityCommands::remove_by_id` and one despawn policy: small cleanups of the diff and switch paths, which no shipped code runs, each worth less than the churn in a crate with no other change.
- Batch 12, the test fixture's `is_plugin_added`: api.md's rule governs the stack's plugin groups; the fixture protocol is a test's, added to apps that may or may not run prediction's group.
- Batch 13, the IO thread sends (11-net-stack/packet-session.md, The IO thread sends, planned): it stays planned, as it moves every socket write across threads, which the endpoint runtime's wake and its tests must take, and a measured run of a server of many clients should judge it, past this review's scope.
- Batch 13, `SerializeFns` to two monomorphic functions: the custom encodings serve a test of the registry's erasure, and the erased function pointers serve every message's registration; a rewrite of the registry's erasure is a refactor of net-messages with no defect behind it.
- Batch 13, one RFC 9000 §A.3 decoder: the two decode widths saturate differently, a packet number's 62 bits and a tick's 32, each beside its tests; a shared function would take both as parameters for two short functions.
- Batch 14, `Measured` made by a patch mechanism: the workspace has none; each `*Patch` is a hand-written twin too, and a macro for one pair of 19 fields cuts about 40 lines, below the guide's bar for a macro.
- Batch 16, one creep AI and one tower AI in a rules package: a unit type's `orders.ai` names only its own package's script, so a shared AI needs a capability feature; and lane's edge-to-edge reach does not carry to the MOBA and proving copies, whose towers are boxes, whose `radius` scripts read as `()`. Both went to the issue log.
- Batch 18, the host's events: every one of the 34 events has its round-trip test already, beside it; the 11 folders hold tests the rule of 40 % moves out of their files, so a table test would only gather them.
- Batch 18, the append writer's clock: the watchdog measures a real sync, and a clock passed through the store's bounds would serve 450 ms of two tests, each within its bound; the stream writer's 20 ms shows that a caller does not put, an absence no rendezvous shows.
- Batch 18, the import's fixture in a `OnceLock`, and the fog test's unit search: nextest runs each test in a process of its own, so a `OnceLock` shares no import, and each import is a case of its own; the fog test's search is `MatchUnits::all` with its own filter.
- Batch 18, the local server's wall clock and the socket's 400 ms: the local server's test drives its real thread, which a hand-stepped time would need a seam in the server for; a shorter idle timeout than 200 ms lets a loaded runner's stall end the link.
- Batch 19, `ActionSlotIndex`: a player slot is a `PlayerSlot` or a `u32` and an action slot a `u8` of `ActionSlots`, so the types already tell them apart, and a newtype at 68 parameters, their callers, the script API and the wire catches no mix; the callback table's `slot` is a `place`.
- Batch 19, `ServerPhase::now` and the restore's picks: `NextState`'s variants are Bevy's public API; the picks a host asks for are the dedicated server's command line, which only its opening holds, and the harness restores with none.
- Batch 19, the append writer's join: Rust drops a struct's fields after its `Drop::drop`, which the worker field's comment states; an `Option` to join by hand adds a state the type does not need.
