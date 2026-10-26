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

### 1. In-flight actions and despawns (core actions, abilities, items, units)

Files: `capabilities/core/actions/src/action_slots/{mod.rs, action_progress/mod.rs}`, `capabilities/abilities/src/{channels.rs, casts.rs}`, `capabilities/core/units/src/unit_ticks.rs`, tests in `capabilities/capabilities/tests/abilities`.

- [ ] `action_progress/mod.rs:565-566` `ActionProgress::fits` asks `slot.rank == Some(at)` of a channel. A learn during the channel (`progression/src/learn_orders.rs:32`, `abilities/src/abilities_effect.rs:53`) then makes state that a restore refuses with a breach. The `started` check of an order has the same defect. → `fits` asks `slot.action == Some(id)` and `book.is_ranked(id, at)`, since ranks only rise. Add a test: a learn during a channel, then a snapshot that restores.
- [ ] `abilities/src/channels.rs:282-291` takes the tick interval from the slot's current rank, but the hooks and the hold take the captured rank. → Read the rule from `InProgress::Channel { action, rank }`, as `channel_hold` does. Add a test of the interval after a learn.
- [ ] `action_slots/mod.rs:387` `SlotsMut::fill` promises to stop what is under way. It calls `slot_emptied` (`action_progress/mod.rs:511-522`), which calls `interrupt` and sends an order back to `Ordered`. A refilled slot (`items/src/inventory.rs:202`) then starts the new action at the old target. → `slot_emptied` ends an order of that slot and cuts its channel. `interrupt` keeps its meaning for blocks. Add a test: an order whose slot a replace refills.
- [ ] `units/src/unit_ticks.rs:41-58` `end_lifespans` despawns a due unit that is dead, with no regard for `Kept`, and can queue a second despawn after `combat/src/dying.rs:100-138`. → End only the living (`Without<Dead>`). Add a test: a summon that dies in the last tick of its lifespan and is kept.
- [ ] `abilities/src/casts.rs:107,118` keeps a `present` query to forget gone units. `OrderSteps::forget_gone` (`progress_dues.rs:100`) already does that on `Remove<ActionProgress>`. → Delete the query and its branch.
- [ ] `action_slots/mod.rs:69-73` `charge_rule` panics, while `values_in` and `kind_in` return `None` for an action that the book does not hold. → One rule for the three.
- [ ] `action_progress/mod.rs:237`: `halt` says "as `stop` does", but no `stop` exists. → Name `end`.

### 2. Journal publication and restore (protocol, host)

Files: `session/protocol/src/session_log/{publish.rs, mod.rs}`, `session/host/src/server/session_dir/mod.rs`, tests in both.

- [ ] Bug: `session_dir/mod.rs:301-353` `publish_into` stores a failed journal read in `failed`, which `SessionLog::publish` cannot see (`publish.rs:20`, `JournalWalk`). `DurableFile::write_with` renames the truncated log into place, and only then is `failed` checked. `SessionDir::find` takes the session as published, so the next start neither restores it nor aborts it. → `JournalWalk` returns `Result<(), JournalStreamError>`, `publish` returns it as a `PublishWriteError` case, and the fill fails before the rename. Add a test: a walk that fails part of the way through publishes no file.
- [ ] `session_dir/mod.rs:201` collects `Vec<&[u8]>` of the records, 16 B per record, about 1.9 GB at `MAX_JOURNAL_LEN`. `session_dir/mod.rs:186-195` reads the whole journal. → A `JournalReplay` builder in protocol (`from_header`, `apply`, `finish`), fed by `JournalFrames::stream`. Peak memory becomes one record plus the log.

### 3. Peer-fed work and authenticity (net-channels, net-packets, net-udp, net-stack)

Files: `network/net-channels/src/{gap_store.rs, channel_receiver/mod.rs}`, `network/net-packets/src/{packet_session/mod.rs, stateless_reset/mod.rs, new_reno.rs, sent_packets/mod.rs, endpoint_core/mod.rs, establish.rs}`, `network/net-udp/src/address_token.rs`, `network/net-stack/src/protocol_check_plugin/`.

- [ ] `channel_receiver/mod.rs:299,305` admits ids up to `pending + 32767`, while an honest sender stays below `pending + SEND_WINDOW` (`reliable_send_state.rs:19`). `gap_store.rs:75-100` inserts into a sorted `VecDeque`, so a peer that orders its ids moves about 2.7 GB, 0.3 to 0.5 s of frame-thread CPU per link. → Refuse ids past the send window as `ProtocolViolation`, and give `GapStore` O(log n) inserts. Add a test of an adversarial order.
- [ ] `packet_session/mod.rs:399` ends a closing session on an unauthenticated Close header (RFC 9000 §10.2.2 drains only on an authenticated close). → Open the packet with the session's seal first. Add a test of a spoofed Close.
- [ ] `stateless_reset/mod.rs:21` and `address_token.rs:96` hand-roll constant-time compares with no `black_box`. → Hold the reset token and the tag as `campfire_common::Secret<16>` (`endpoint_core/mod.rs:96`, `establish.rs:92`) and compare with `==`.
- [ ] `new_reno.rs:60` truncates `MAX_DATAGRAM * bytes / window` for each ack: 17 % slow with small packets, and no growth past a 240 KB window. → Keep `bytes_acked` and add `MAX_DATAGRAM` each time it reaches the window, as quinn does. Add a test with small packets.
- [ ] `sent_packets/mod.rs:240` counts losses sent before the first RTT sample toward persistent congestion (RFC 9002 §7.6.2). → Keep the time of the first sample and skip what was sent before it.
- [ ] `protocol_check_plugin/mod.rs:44,80` sends the check as a registered event, whose ids shift in a mismatched build, and sends it on `Add<Connected>` together with data, against transport.md "Both peers agree on the protocol before data flows". → The ALPN names the app and the hash of the three registries, alert 120 maps to `ProtocolMismatch` in `net-udp/src/alert/mod.rs`, and `ProtocolCheckPlugin` and its event go (11-net-stack/transport.md, The hashes travel in the ALPN, planned). Add a wire test with two different registries.

### 4. Local-tick state in prediction (net-prediction, net-replication)

Files: `network/net-prediction/src/{rollback_check_state.rs, prediction_despawned.rs, locally_predicted.rs, rollback_plugin/mod.rs, prediction_plugin.rs, prediction_despawn_command/mod.rs, prediction_setup/mod.rs, prediction_history/mod.rs, tick_period_history/mod.rs, prediction_diagnostics_plugin.rs}`, `network/net-replication/src/{pre_spawned_receiver.rs, confirmed_history/mod.rs}`, `network/net/src/sim_client/mod.rs`.

- [ ] Bug: `rollback_check_state.rs:54,57` (`local_despawn`, `local_skip_despawn`) and `prediction_despawned.rs:14` hold local ticks that `LocalTimelineShift` does not move, against its contract (`net-timeline/src/local_timeline_shift.rs:6`). They are compared with shifted ticks at `rollback_plugin/mod.rs:155,597,741-790` and `prediction_plugin.rs:210`. → One `RollbackSpawns` in net-prediction (`insert`, `drain_after`, `forget_through`, `shift`) that takes both the pre-spawns and the local predictions. Replication keeps only `unmatched` (`pre_spawned_receiver.rs:19,48,80`). Add a test: a shift, then a rollback.
- [ ] Bug: `prediction_despawn_command/mod.rs:36-60` stamps `LocalTimeline::tick()` outside `FixedMain`. A rollback enables the entity again, and no replay despawns it. → Stamp `simulated_tick()`. Outside `FixedMain`, despawn at once or assert, as the pre-spawns do.
- [ ] Bug: `rollback_plugin/mod.rs:422` unwraps `PredictionCounts`, which only `PredictionDiagnosticsPlugin` creates (`prediction_diagnostics_plugin.rs:76`). → `RollbackPlugin` creates it.
- [ ] `prediction_setup/mod.rs:24-32`: the rollback window, the minimum of two settings, is computed again at each reader (`prediction_history/mod.rs:207,231`, `rollback_plugin/mod.rs:485,686,798`, `prediction_plugin.rs:193,215`, `tick_period_history/mod.rs:59-66`, `net/src/sim_client/mod.rs:754-787`). → One `RollbackWindow` resource, set at finish.
- [ ] `confirmed_history/mod.rs:159` `ConfirmedHistory::shift` has only a test caller, and server ticks never shift. → Delete it and its test.
- [ ] `rollback_plugin/mod.rs:424-425` clamps with `max(0)`, but `RollbackScan::request` (`rollback_scan.rs:71`) already refuses a negative delta. → `debug_assert!`.

### 5. Vision's reveals and maps (vision)

Files: `capabilities/vision/src/{fog/mod.rs, seeing.rs, sight_maps/counted_maps.rs, vision_effect.rs, reveals.rs, vision_grid.rs, vision_books/mod.rs, lib.rs}`, `capabilities/items/src/lib.rs`, `capabilities/capabilities/tests/alloc/main.rs`.

- [ ] Bug: `counted_maps.rs:19,68` counts in `u16`, and `vision_effect.rs:22-36` adds reveals with no bound. A mode script that reveals one spot each tick passes 65,535, and `overflow-checks` in release panic the server. → A bound on the reveals that one team holds, with an `ApiError` past it. Add a test at the bound.
- [ ] `fog/mod.rs:120-129,178-185` and `seeing.rs:73` take back and count again every live reveal each tick, about 628 updates per tick for a 10 m reveal on a 1 m grid, against vision.md "A stage that costs what changed". → Count a reveal's runs when it arrives and take them back when it expires.
- [ ] `vision_grid.rs:16` and `vision_books/mod.rs:31-62` let a map have 2²² cells and 64 teams, about 1 GiB of counts on a server. → A load rule on cells × teams against a stated budget, `VisionDataError::TooLarge`.
- [ ] `vision/src/lib.rs:109` says "on combat", but `NEEDS = []`. `items/src/lib.rs:119` describes the empty book that "Books before the install" refuses. → Correct both docs.
- [ ] `tests/alloc/main.rs:59-64` audits no allocation of vision's fog, progression's experience share or items' `want_items`. → Extend `Battle` with a vision grid and an experience share.

### 6. Untrusted reads and the Zero Hour check's trust (store, import, zero-hour-check)

Files: `base/store/src/input_file/mod.rs`, `apps/import/src/zero_hour/{format/big/big_archive.rs, import_names.rs, mod.rs}`, `checks/zero-hour-check/src/{import_stamp.rs, lib.rs, import_check.rs}`.

- [ ] `input_file/mod.rs:192-196` `read_at` allocates `vec![0; len]` before it checks the range. `big_archive.rs:39-43` passes it an untrusted `header_len` of up to 4 GiB. → `read_at` checks the range before it allocates, and the archive refuses `header_len > file.len()` as `ArchiveError::Short`.
- [ ] Bug: `import_stamp.rs:30-52` keys the kept import on the install's version and `OUTPUT_VERSION` only. The rules package is `include_bytes!` (`rules_package/mod.rs:45-62`), so after an edit to `bot.rhai`, `mode.rhai` or `rules.toml` the check plays the old rules. → The stamp also holds the fingerprint of `packages/zero-hour/package.index`, and `kept` requires it to be equal.
- [ ] `lib.rs:115-118` pins the fingerprints that the stamp records, not those of the packages it loaded. → Pin `check.packages.fingerprint()` and the rules dependency's fingerprint, and fail when either differs from the stamp.
- [ ] `import_names.rs:124-129` reports a projectile name that a placed type holds as `NameClash { first: object.name, second: object.name }`. → A variant of its own that names the projectile and the placed type.

### 7. Match start (navigation)

Files: `capabilities/navigation/src/{room_making.rs, lib.rs, body_index/}`, `capabilities/mode/src/lib.rs`.

- [ ] `room_making.rs:63` runs the whole static tracking for each static spawn: it gathers and sorts every static body, diffs the `BodyIndex` and rebuilds its `RowIndex`. `Mode::start` (`mode/src/lib.rs:476`) spawns the map's units one at a time, so a start is O(S²), with S up to 2,683 on a Zero Hour map. → `BodyIndex` takes one added body, which merges into the index. Add a bench of a start with 2,000 static bodies.

### 8. The load once (package, capabilities, import, zero-hour-check)

Files: `session/package/src/mode_packages/{mod.rs, mode_read.rs}`, `capabilities/capabilities/src/release/{mod.rs, book_build.rs, api_binder.rs, hook_list.rs, engine_tag_list.rs, called_registries.rs, section_list.rs, module_sets.rs, kinds_only.rs, state_only.rs}`, `capabilities/capabilities/src/books/{load_names.rs, package_content/mod.rs}`, `session/package-format/src/package_reader.rs`, `apps/import/src/zero_hour/mod.rs`, `checks/zero-hour-check/src/{import_check.rs, lib.rs, bench.rs}`.

- [ ] `mode_read.rs:81-88` runs a full `ModePackages::new` (`mod.rs:343-459`) in each `on(map)`: the API binding, 760 KB of TOML, every script compiled and every book built. The import calls it for 150 maps (`zero_hour/mod.rs:303-308`), which parses about 114 MB of TOML and compiles 300 scripts. → `ModeRead` keeps the part that names no map, and `on(map)` reads the map's files and runs the checks that depend on them.
- [ ] `import_check.rs:434`, `lib.rs:109` and `bench.rs:106` call `ModePackages::at` once, then once more for each of the 8 plays. → One `ModeRead` held by `ImportCheck`, and `read.on(Some(map))` for each play.
- [ ] `book_build.rs:15-82`: `check_rules`, `books` and `unit_types` each make a `BookCx`, and `LoadCheck::run` calls the first two on the same input. `tag_names` (`mode_packages/mod.rs:577`) makes a third. → One `Release::book_build(&BookInput) -> BookBuild`, whose `check_rules()` borrows the cx and whose `finish()` consumes it.
- [ ] The seven projections in `release/{hook_list, engine_tag_list, called_registries, section_list, module_sets, kinds_only, state_only}.rs` each implement every method of `Declarations` to collect one kind (about 970 lines, about 280 instantiations). `release/mod.rs:161-190,222,452-477` walks again at each accessor. → One `DeclarationCatalog`, which implements every method and is built once for each `CapabilitySet`. The `Release` accessors read it.
- [ ] `api_binder.rs:76-86` records every section and listed key as `Status::Runs(ApiVersion::FIRST)` by hand. → `Section::STATUS` and `Listed::STATUS` as trait consts with no default.
- [ ] `release/mod.rs:137-151`: the doc of `capability_set` promises an error, but the code uses `expect`. → Correct the doc.
- [ ] `load_names.rs:117-118` scans a `BTreeSet` with `iter().any`. → `contains`. `package_content/mod.rs:69` `section_or_default` has no caller. → Delete it.
- [ ] `package_reader.rs:34-66`: each `open` lists every directory of the package (314 directories) and maps every row. → A SHA-256 to paths map from the index, built with no I/O, and a resolve for each read. Skip this item if the zip packages of `.notes/package-archive.md` land first.

### 9. The session on disk and its lifecycle (host, runner, protocol, verifier, client, net)

Files: `session/host/src/{server/session_dir/mod.rs, server/snapshot_dir.rs, server/checkpoints/mod.rs, server/checkpoint_thread.rs, server/reports.rs, messages/save_command.rs, error.rs, report_dir/}`, `session/runner/src/{session/mod.rs, session/session_world.rs, replay/mod.rs}`, `session/protocol/src/session_log/{mod.rs, file.rs}`, `apps/verifier/src/{lib.rs, verification/mod.rs, report_replay/mod.rs}`, `apps/client/src/{connection.rs, command_line/mod.rs}`, `network/net/src/sim_server/sim_server_world/mod.rs`.

- [ ] Each of `client/src/connection.rs:94-104`, `verifier/src/report_replay/mod.rs:73-75` and `verifier/src/verification/mod.rs:68` reads and decodes a log file. → `ReportDir::read_log` and one host read of a log file, with the release check.
- [ ] No caller checks a report's release (`report_replay/mod.rs:71-84`, `client/src/command_line/mod.rs:241-262`), so a report from another release fails on its packages. → `ReportDir::read_facts` refuses another release, as `verify_file` does.
- [ ] `verification/mod.rs:36-46` `SnapshotFiles` repeats `SnapshotDir::read` (`snapshot_dir.rs:24-29`). → `impl SnapshotSource for SnapshotDir`.
- [ ] Three places choose between a start at tick 0 and a resume: `session_dir/mod.rs:420-455` (two identical `SessionSource` literals), `runner/src/replay/mod.rs:36-87` and `verification/mod.rs:84-100`. → `Session::open(world, source, Option<ResumeAt>)` and `Replay::new(published, packages, Option<ResumeAt>)`.
- [ ] `session_world.rs:96-110` and `:270-279` both switch to a segment's seed. → `Session::draw_from(world, segment)`. `session_world.rs:112-128` writes the `TickInput` push loop twice. → One helper.
- [ ] `checkpoint_thread.rs:151` and `session_world.rs:205` both build the `Checkpoint` record. The snapshot fingerprint is hashed twice in a load (`session_world.rs:266` and `session/mod.rs:237`) and twice in `take_again` (`session_world.rs:209` and `snapshot_dir.rs:18`). → `CheckpointBegun::record(state_hash, fingerprint)`. `SnapshotDir::write` takes the fingerprint. `restore_checked` splits into the fingerprint check and the restore.
- [ ] `session_dir/mod.rs:491-494` and `sim_server_world/mod.rs:189-193` find the last seed through `revealed_seeds().seed(segment)`. → `Session::last_seed()`.
- [ ] A report travels as a save: `SaveCommand::Report` (`save_command.rs:9`), routed through `Checkpoints::take` to `SaveTaken::Planned` (`checkpoints/mod.rs:144-152`), refused as `SaveRefusal::NoData` (`reports.rs:70-77`). → `JournalCommand` and `JournalRefusal`, and a host dispatcher that sends a save or a load to `Checkpoints` and a report to `Reports`.
- [ ] `runner/src/session/mod.rs:446,452`: `record_checkpoint` and `record_result` sign, while every other `record_*` checks a signature. → `serve_checkpoint`, `serve_result`. `session/mod.rs:109` `SessionRole::server()` returns a tuple. → A `ServerRole { signer, journal }` struct.
- [ ] `session_log/mod.rs:341,400-408,453,942,1062,1168,1195,1309` compute the file's length by hand, with a magic `+ 6`. → A `FileLen` type in `file.rs` beside the encoder.
- [ ] `session_log/mod.rs:347` `applied_inputs` and `replay/mod.rs:102` `run_segment_each` have only test callers, and the docs of `run_segment_each` and `verifier/src/lib.rs:1` describe a segment-by-segment replay that the verifier does not do. → Gate both under `internals`, and correct the docs.
- [ ] `session_dir/mod.rs:270,293` `publish_reports` removes and inserts `Reports` each frame. → `world.resource_scope`. `server/bot_driver.rs:167`: the doc of `max_payload_len` describes `send`, and `session/mod.rs:423` cites `SessionLog::full` (the method is `is_full`). → Correct both.

### 10. Per-tick costs of stats and bots (stats, capabilities bot, bots, zero-hour scripts)

Files: `capabilities/stats/src/{player_grants.rs, upgrade_grants.rs, lib.rs, refresh.rs, live_carriers.rs, modifier_hooks.rs}`, `capabilities/capabilities/src/bot/{bot_call.rs, bot_api.rs}`, `session/bots/src/{thinking_bot/mod.rs, bot_script/mod.rs, tick_bots.rs}`, `capabilities/core/players/src/order_units.rs`, `packages/zero-hour/scripts/bot.rhai`.

- [ ] `player_grants.rs:53-120` and `upgrade_grants.rs:43-128` make the same O(N) scan one after the other in `GrantAtResolve` (`lib.rs:384`). → One producer that pushes both grants of each visited unit.
- [ ] `refresh.rs:107-111` calls `LiveCarriers::forget` (`live_carriers.rs:60-71`) in each of the 10 gaps of a tick, and it does an index lookup for each carried change even when nothing was visited. → Return at once when no unit was visited and none is gone, and prune gone units only on despawn.
- [ ] `bot_call.rs:108-112` `BotCall::param` searches by string and converts with `ModeParam::to_value()` on each read. → Keep the converted table, converted again only for a params `Arc` it has not seen, as `ModeScript::param_named` (`mode/src/mode_script.rs:41`) does.
- [ ] `bot_api.rs:122-133` `order` converts a map to a `TomlValue`, reads it with serde and discards both errors with `.ok()`, so the bot author sees only `NotADirective`. → `NotADirective` carries the codec error, and the read takes the `ScriptValue` directly.
- [ ] `thinking_bot/mod.rs:124` collects a `Vec` for each think, `bot_script/mod.rs:206-226` boxes the units and clones the directive for each order, and `:310-312` boxes a slice through `OrderUnits::new` (`order_units.rs:27`), while `tick_bots.rs:15` claims that no input allocates. → Buffers kept on `BotBuffers`.
- [ ] `bot_script/mod.rs:205-226` drops an order to units that the bot does not own as "unsent", while `may_start` and `idle` fail the call for the same input. → One rule.
- [ ] `bot.rhai:128-131,145-172,176-201`: `train` calls `ctx.own(unit).len()` for each producer × army type, `build` calls `ctx.own` twice for each building, and `gather` and `train` each build the producers list. → One `producers` helper and one count of own units by type for each think.
- [ ] `modifier_hooks.rs:118-120` finds the same instance twice. → One lookup that returns the place and the `Carried`.

### 11. Shared containers and contracts below the capabilities (common, sim, units, combat, projectiles, mode, stats, production, navigation, orders, progression)

Files: `base/common/src/sorted_set.rs` (new), `sim/sim/src/due_index/`, `capabilities/projectiles/{Cargo.toml, src/struck_units.rs, src/homers.rs, src/projectile_books.rs}`, `capabilities/combat/src/{recent_attackers.rs, clip_shots/mod.rs}`, `capabilities/mode/{Cargo.toml, src/victory.rs, src/lib.rs}`, `capabilities/stats/src/player_modifiers.rs`, `capabilities/core/units/src/living_spawns.rs`, `capabilities/progression/src/{experience.rs, track_data/mod.rs}`, `capabilities/navigation/src/routing.rs`, `capabilities/production/src/{order_dues.rs, construction/mod.rs}`, `capabilities/orders/src/attack_move.rs`, `capabilities/core/actions/src/action_slots/action_progress/progress_dues.rs`.

- [ ] Nine sorted unique `Vec` states (`struck_units.rs:26,59`, `homers.rs:43`, `recent_attackers.rs:30,57`, `clip_shots/mod.rs:44-64`, `victory.rs:195`, `player_modifiers.rs:22,53`, `living_spawns.rs`, `experience.rs`, `track_data/mod.rs`) each write their binary search and their order check. Some check in `Deserialize`, others in `SimComponent::check`. → `SortedSet<T>` and `SortedMap<K, V>` in `common`, with a `Deserialize` that checks.
- [ ] The standing-order loop on a due index is written in `routing.rs:151-161`, `order_dues.rs:35-47`, `construction/mod.rs:355`, `attack_move.rs:66-77` and `progress_dues.rs:146,158`. → `StandingOrder` moves into `sim` beside `DueIndex`, and each owner implements it.
- [ ] `projectiles/Cargo.toml:20,35` depends on `campfire-stats` only for `MoveSpeedCap` (`projectile_books.rs:7`). → `MoveSpeedCap` moves to `units`.
- [ ] `mode/Cargo.toml:20,39` depends on `campfire-production` only for `.after(ProductionSystems::Finish)` (`mode/src/lib.rs:24,211`). → A contract set in `units` that production's `Finish` joins and mode orders after.

### 12. The sync layers' shape (net-timeline, net-sync, net-interpolation, net-frame-interpolation, net-prediction, net-replication, net-core, net-stack)

Files: `network/net-timeline/src/{timeline.rs, timeline_setup.rs, network_timeline.rs, timeline_systems.rs}`, `network/net-sync/src/{remote_timeline.rs, local_timeline_sync/mod.rs, local_timeline_sync_plugin.rs, timeline_sync.rs, link_estimator_plugin.rs}`, `network/net-interpolation/src/{interpolation_setup.rs, interpolation_state.rs, interpolation_timeline/mod.rs, interpolation_registry/mod.rs, rule_building.rs, registered_rule.rs, interpolation_plugin/mod.rs}`, `network/net-frame-interpolation/src/{frame_registry/mod.rs, frame_registry_plugin.rs}`, `network/net-prediction/src/{prediction_registry/mod.rs, correction/mod.rs, timeline_switch_plugin/mod.rs, fixtures/protocol.rs}`, `network/net-replication/src/{history_diff_receiver/mod.rs, confirmed_tick_map/mod.rs, deferred_entity_commands/mod.rs}`, `network/net-core/src/`, `network/net-stack/src/layers/mod.rs`.

- [ ] `timeline.rs:12`, `timeline_setup.rs:2` and `network_timeline.rs:13-57` carry nothing generic: `NetworkTimeline::Setup` is never read, and no code is generic over it. Two workarounds exist only for the generic: `RemoteTimelineSetup` (`remote_timeline.rs:53-61`) and `InterpolationSetup` (`interpolation_setup.rs:53`). → Plain `RemoteTimeline { now, estimate }` and `InterpolationTimeline { now, state }`, with `TickInstant::advance`.
- [ ] `local_timeline_sync/mod.rs:26-35,189-209` and `interpolation_state.rs:6-11` hold the same fields, which `timeline_sync.rs:95-111` reaches through six accessors. → A `SyncController` that both embed. `local_timeline_sync_plugin.rs:112-113` calls `set_synced(false)` after `reset()`, which already does that. → Delete the call.
- [ ] `remote_timeline.rs:37` `first_estimate` always equals `last_received.is_none()`. → Delete the field. `timeline_systems.rs:10` `IncrementLocal` has nothing that orders against it. → Delete it.
- [ ] The three registries each hand-roll a sorted `Vec<(ComponentTypeId, V)>` (`prediction_registry/mod.rs:95-135`, `interpolation_registry/mod.rs:148-173`, `frame_registry/mod.rs:198-224`). Two define `PendingRules` (`interpolation_registry/mod.rs:53`, `frame_registry/mod.rs:62`), `matches_filter` is copied (`registered_rule.rs:50`, `frame_registry/mod.rs:232`), and each finds "this app runs the pipeline" in its own way (`prediction_registry/mod.rs:79-90`, `frame_registry_plugin.rs:22`, `rule_building.rs:96`). → `BTreeMap`, one `DeferredInstalls` in net-core, one `matches_filter`, and the pipeline plugin's `finish` installs for all three.
- [ ] `link_estimator_plugin.rs:16` puts a `LinkEstimator` on every server link, and the shared `SyncPlugins` (`layers/mod.rs:56`) samples it each frame, but nothing on a server reads it. → Require it on `Client` only, in `ClientSyncPlugins`.
- [ ] `history_diff_receiver/mod.rs:462-479` and `:65-83` compute the offset and `TooManyDiffs` twice, `:431` and `:447` check the cursor twice, and `queue_diffs` (`:420`) has one caller. → One `PendingDiffMessage::ending_at` and one entry point. `confirmed_tick_map/mod.rs:207-211,237-243` compares twice. → `ConfirmedTick::is_newer_than`.
- [ ] `DeferredEntityCommands` has no `remove_by_id`, so `timeline_switch_plugin/mod.rs:225-244` mixes two queues on one entity (two archetype moves). → Add `remove_by_id`. `DespawnReplicated` is queued both silenced (`rollback_plugin/mod.rs:763`, `interpolation_plugin/mod.rs:130`) and with a warning (`prediction_plugin.rs:209`, `pre_spawned_receiver.rs`). → One rule. `correction/mod.rs:391-395` checks `is_none()` before a `let-else`. → The `let-else` alone.
- [ ] `fixtures/protocol.rs:88` adds `FrameInterpolationPlugin` under `is_plugin_added`, which api.md refuses. → The test app lists it.

### 13. The transport's surface (net-endpoint, net-messages, net-bytes, net-channels, net-packets, net-payload, net-stepper, net-stack, docs)

Files: `network/net-endpoint/src/{endpoint_plugin.rs, endpoint/mod.rs}`, `network/net-packets/src/endpoint_runtime/`, `network/net-socket/src/udp_io.rs`, `network/net-stack/src/layers/mod.rs`, `network/net-transport/src/fixtures/transport_stack.rs`, `network/net-messages/src/{fixtures/messages_stack/mod.rs, serialize_fns.rs, app_message_ext.rs, app_event_ext.rs, send_queue.rs, message_plugin/mod.rs, multi_message_sender/mod.rs, message_registry/tests.rs}`, `network/net-stepper/src/connection_stack/mod.rs`, `network/net-bytes/src/{wrapping_id.rs, writer/mod.rs, byte_arena/mod.rs, reader.rs}`, `network/net-channels/src/{message_id/mod.rs, tick_decoder/mod.rs}`, `network/net-packets/src/packet_numbers/mod.rs`, `network/net-payload/src/send_payload.rs`, `docs/design/11-net-stack/testing.md`.

- [ ] `endpoint_plugin.rs:31` adds `PacketSessionLinkPlugin` under `is_plugin_added`, which api.md refuses. The test stacks do the same (`transport_stack.rs:17,36`, `messages_stack/mod.rs:17,36`, `connection_stack/mod.rs:23,43`). → List it once in `layers/mod.rs::shared`, and each test stack lists what it needs.
- [ ] `net-endpoint/src/endpoint/mod.rs:215-243`: on the OS clock the frame makes one `sendmsg` for each peer in `send_from_frame`, about 1.5 to 3 µs each, so the frame's cost grows with its players. → The frame queues the sealed datagrams and wakes the IO thread, which sends them (11-net-stack/packet-session.md, The IO thread sends, planned).
- [ ] `serialize_fns.rs:96`, `app_message_ext.rs:34` and `app_event_ext.rs:30` take custom encodings, which only `message_registry/tests.rs:115` uses, through five layers of function pointers and an `Arc<dyn Any>`. api.md says "One encoding of a message and a component". → Two monomorphic functions, mapped or not, picked at registration. Delete the custom test.
- [ ] `wrapping_id.rs:8` exports a macro for one type (`message_id/mod.rs:4`). → A plain `MessageId` in net-channels.
- [ ] `writer/mod.rs:32,49,76`, `byte_arena/mod.rs:48,135` and `reader.rs:49` have only test callers, and `writer/mod.rs:71` `reset` has only the bench. → Delete them, and gate `reset` under `internals`.
- [ ] `packet_numbers/mod.rs:18` and `tick_decoder/mod.rs:41` both write RFC 9000 §A.3. → One `const fn` over u64 with a width in bits, in net-bytes.
- [ ] `send_payload.rs:4` and `send_queue.rs:13` are aliases that only rename. → `Bytes` and `f32`.
- [ ] `multi_message_sender/mod.rs:94-101` and `message_plugin/mod.rs:424-431` `debug_assert!` a send against the message's direction, then drop it in release. → `assert!`, because it is misuse of the public API.
- [ ] `send_queue.rs:85` and `message_plugin/mod.rs:421` describe failures that no longer exist. → Delete both comments. `testing.md:48` says the window is 32, but it is 64. → Cite packet-session.md.

### 14. Client files and the import's internals (client-files, import, zero-hour-check, package-disk)

Files: `session/client-files/src/{client_units/mod.rs, map_lights.rs, ktx2_image.rs}`, `apps/import/src/zero_hour/{mod.rs, import_names.rs, model_writer.rs, map_import/mod.rs, format/look_conditions.rs, format/map/terrain/mod.rs, terrain_atlas/mod.rs}`, `apps/import/src/{main.rs, gltf/glb_builder.rs, texture/tga_file.rs}`, `checks/zero-hour-check/src/{import_check.rs, golden.rs}`, `session/package-disk/src/package_writer/mod.rs`, `session/package-format/src/package_walk.rs`.

- [ ] `client_units/mod.rs:19-25` and `map_lights.rs:15-18` have public fields and check their rule only in `Deserialize`, so the import can write a file that the client refuses. → Private fields and a `new` that runs the same check, as `TerrainFile` does.
- [ ] `zero_hour/mod.rs:259-260` writes `client/units.toml` with `Toml::write`, against 02-engine-core.md "A client file declares its format". → `looks.encode()`. `import_check.rs:79,94-95,146,204` names the format of four client files. → One helper, `F::decode(&files.read_file(path)?)`.
- [ ] `import_names.rs:22-24` keeps `placed` and `fields` as two maps with the same keys, and `:80-83` clones a whole `ImportedType` for each placement. → One map of one entry struct, and `NamedType` borrows.
- [ ] `model_writer.rs:616` depends on the map stage for `condition_names` (`map_import/mod.rs:413-420`). → `LookConditions::names()`.
- [ ] `format/map/terrain/mod.rs:91` writes 28, and `terrain_atlas/mod.rs:58` derives `2048 / 72`. → A `const` assert that ties them.
- [ ] `main.rs:54-129` and `import_check.rs:171-178,210-216,247-252,299-305` each write a message for each loss kind, and each check group repeats filter, count, pin and log. → `ImportLoss: Display`, and one helper that counts by cause.
- [ ] `golden.rs:281-327,369-391` keeps `Golden` and `Measured` as twins of 19 fields. → `Measured` made by the workspace's patch mechanism.
- [ ] Copies on the import's write path: `package_writer/mod.rs:47` hashes a slice through `Read` (one more copy of 1.8 GB), `glb_builder.rs:323-421` makes a `Vec` for each accessor and does not reserve the output, and `ktx2_image.rs:126-192` does not reserve a known length. → `PackageWalk::hash_bytes`, writes straight into the buffer, and `with_capacity`.

### 15. Typed data between the import and the scripts (mode, import, zero-hour package)

Files: `capabilities/mode/src/mode_data.rs`, `apps/import/src/zero_hour/{map_import/mod.rs, rules_package/mod.rs}`, `packages/zero-hour/scripts/{mode.rhai, bot.rhai}`.

- [ ] `map_import/mod.rs:66-76,296-308` parses each waypoint into `SkirmishPoint::Start(n)` or `Rally(n)`, then writes only its name. `mode.rhai:30,37,69-77` and `bot.rhai:288-298` build `` `Player_${n}_Start` `` again and compare strings, in two shapes. → A slot number param and a `rally` tag, and one lookup by slot.
- [ ] `rules_package/mod.rs:204-221` puts each faction's data into param names (`start_building_<faction>` and seven more), which the scripts join at run time (`bot.rhai:110,122,128,146,155,178,212`, `mode.rhai:36-51`), so a misspelt name fails at run time and not at the load. `ModeParam` has no table (`mode_data.rs:205`). → `ModeParam::Table`, and `factions.<faction>.{…}` that the load checks.

### 16. Test packages (test packages)

Files: `packages/test/{engine/modes/lane, engine/modes/proving, moba/modes/3v3}/scripts/{creep_ai.rhai, tower_ai.rhai}`, `packages/test/engine/rules/`, the packages' manifests and indexes.

- [ ] `moba/modes/3v3/scripts/creep_ai.rhai:42-44` and `engine/modes/proving/scripts/creep_ai.rhai` still measure `in_reach` centre to centre. Lane's copy measures edge to edge since 19 October, "as `nearest_visible` counts it". The three `tower_ai.rhai` are identical copies. → One creep AI and one tower AI in a rules package that the modes name.

### 17. The rules test (rules, clippy.toml, workspace Cargo.toml, app-threads, design)

Files: `test/rules/src/{every_rule.rs, bans.rs, source_files.rs, source_file.rs, workspace.rs, test_support.rs, errors.rs, unsafe_code.rs, manifests.rs, capabilities/genre_words.rs}`, `clippy.toml`, `Cargo.toml`, about 30 crate roots, `base/app-threads/src/lib.rs`, `apps/client/src/view/{tests.rs, look_states/tests.rs}`, `checks/lan-check/src/command_line/mod.rs`, `docs/design/02-engine-core.md`.

- [ ] `bans.rs:118-143` (`CODEC_METHODS`) and `:337-350` (`UNOWNED_METHODS`) copy `clippy.toml` and lack 15 of its entries, so a deletion of any of them passes the rule. → The rule reads `clippy.toml` and groups each entry by the proxy that its `reason` names.
- [ ] `every_rule.rs:29` (with `manifests.rs:289-317,633-645`, `unsafe_code.rs:34-42`, `test_support.rs:71-73,92-100`, `bans.rs:845-856`, `genre_words.rs:76-89`) runs each rule's samples after its workspace assert, so a rule that the workspace breaks hides its samples. 02-engine-core.md "The rules are one test" says otherwise. → Each rule's samples in a `#[test]` of their own.
- [ ] `source_files.rs:25-34` tests each of about 2,780 files against each of about 1,055 gated modules, with two `format!` for each pair. `workspace.rs:385-392` calls `format!` for each crate for each file, in about eight walks. `source_file.rs:84-102` blanks a copy of the file at each call. → `Workspace::sources()` keeps each file's gate (by ancestor lookup), crate index and both blanked views.
- [ ] Text scans that a design decision refuses: `test_support.rs:77-90` (`env::temp_dir`), `bans.rs:883-899` (`"Time<Real>"`), `bans.rs:819-832` (`app_removals`), `errors.rs:36-50,62-100`. → A `clippy.toml` ban of `std::env::temp_dir`, `NamedPath`, `file.production()`, and a syn visitor. `lan-check/src/command_line/mod.rs:67` falls back to `env::temp_dir()`, which the new ban then holds.
- [ ] `unsafe_code.rs:7-42` scans for `unsafe` by hand. → `unsafe_code = "warn"` in `[workspace.lints.rust]` and `#![expect(unsafe_code, reason = …)]` in the 9 modules.
- [ ] `02-engine-core.md:204` names cargo's `unused_dependencies` as an enforcer, but no configuration turns it on. → A rule that each dev-dependency is named by its crate's test code, and the design corrected.
- [ ] About 30 crate roots hold the same `#![allow(clippy::needless_pass_by_value, …)]`. → One workspace lint entry, with its reason in a comment.
- [ ] `clippy.toml` bans only `TaskPoolPlugin` and `TaskPoolOptions`, and `client/src/view/tests.rs:326,490` and `look_states/tests.rs:63` start a pool with one thread for each core. → `app-threads` gives the test pool, and `clippy.toml` bans `TaskPool::new` and the `get_or_init` of each pool.

### 18. Test support and the harness (script, orders, capabilities fixtures, harness, net-harness, test-packages, store, host, client, verifier, import, net, net-prediction, net-socket)

Files: `capabilities/core/script/src/scripts/script_failures.rs`, `capabilities/orders/src/lib.rs`, `capabilities/abilities/src/lib.rs`, `capabilities/mode/src/lib.rs`, `capabilities/capabilities/src/{fixtures/vision.rs, fixtures/orders.rs, fixtures/units.rs, release/mod.rs, release/client_holds.rs, release/tests/mod.rs}`, `capabilities/core/visibility/src/data_kind.rs`, `capabilities/core/units/src/` (`Spawn`), `sim/sim/src/sim_world.rs`, `test/harness/src/{fixed_session/mod.rs, fixed_session/fixed_match.rs, match_units.rs}`, `test/net-harness/src/in_process_match/mod.rs`, `test/net-harness/tests/{fog.rs, prototype.rs}`, `test/test-packages/tests/{moba.rs, flaws.rs}`, `base/store/src/{append_writer/, stream_writer/tests.rs}`, `session/host/src/events/`, `apps/client/src/{test_app.rs, hud/tests.rs, view/, selecting.rs, report.rs}`, `apps/verifier/tests/{binary.rs, session_logs.rs}`, `apps/import/src/zero_hour/tests.rs`, `network/net/src/local/local_server/tests.rs`, `network/net-prediction/src/{timeline_switch_plugin/tests.rs, correction/tests.rs}`, `network/net-socket/src/server_socket/tests.rs`, `capabilities/interaction/src/use_action_data.rs`.

- [ ] `script_failures.rs:127` has no `Drop` check of failures that no test took, unlike `UnreadCommands` (`sim/src/unread_commands.rs:97`). So `Orders::think` (`orders/src/lib.rs:265-268`, 22 callers) asserts by hand, and the failures come back as `Vec<FailedCall>` (`abilities/src/lib.rs:236`), as `Vec<FailureKind>` (`mode/src/lib.rs:690`) or by a hand assert. → The `Drop` assert under `internals`, one shape for taking failures, and `Orders::think` deleted.
- [ ] Bug in test support: `client_holds.rs:40-49` lists the kinds that a client gets of its own player by hand, and misses `PlayerStateOwner` and `PlayerStateTeam` (`mode/src/player_state.rs:53,59`). → A `DataKind` predicate in `data_kind.rs`.
- [ ] `release/tests/mod.rs:148-196` closes each pair's needs by hand, which `MatchBuilder::build` does already, and builds 234 matches one after another (0.72 s). → Pass the pair, and sweep on `threads::Parallel` with a `threads-required` override.
- [ ] `release/mod.rs:409` imports `fixtures::units::UnitsFixtures`, and `fixtures` imports `Release`. → Move `load_test_modifiers` and `loads_test_modifiers` into `release::internals`.
- [ ] `fixtures/vision.rs:16,27` `set_properties` is an alias of `Units::hold_properties`. `fixtures/orders.rs:13-36` `Orders::hero` (17 callers) wraps `Spawn` and touches no part of orders. → Delete both, and give `Spawn` an `owner(slot)` setter. About 45 test sites write `for _ in 0..n { game.step(); }`. → `SimWorldInternals::steps(n)`, and `sim_world.rs:215` `run_until(Tick)`.
- [ ] `test-packages/tests/moba.rs:38` reads the whole MOBA tree from disk for each flaw, and `flaws.rs:37-55` runs about 380 flaws one at a time (four tables at 0.41 to 0.52 s). `flaws.rs:13` holds a package name or a directory suffix in one string. → Read once, clone for each flaw, index again only the edited packages, sweep on `threads::Parallel`, and an enum for the reference.
- [ ] `fixed_match.rs:100,143,150`, `fixed_session/mod.rs:44,50,148` and `match_units.rs:38,44` take a player slot as `u32` beside `PlayerSlot` (`fixed_match.rs:134`). `in_process_match/mod.rs:958` returns `u64` where `fixed_match.rs:206` returns `Tick`. `fixed_session/mod.rs:50-60` orders the arguments of `with_rules` and `planned` differently, and 8 of 10 `planned` callers pass `InputRules::LAN`. → `PlayerSlot` and `Tick` everywhere, one count type, and `planned(packages, plan)` with a `.rules()` setter.
- [ ] `append_writer/mod.rs:213,218` reads the wall clock, so `append_writer/tests.rs:133,168` sleep 450 ms. → The clock comes in through the bounds. `stream_writer/tests.rs:70` sleeps 20 ms, then looks. → A rendezvous on the gate.
- [ ] `host/src/events/`: 11 folders hold one round-trip test each, and about 23 events have none. → One table test in `events/mod.rs`, and the folders flattened.
- [ ] `hud/tests.rs:290`, `view/unit_models/tests.rs:51,77-104`, `view/package_source/tests.rs:190`, `selecting.rs:218` and `view/client_data/tests.rs:192` build a bare `App::new()`, against the client decision that a test builds through `test_app`. `view/client_data/mod.rs:217` holds a gated `impl` outside its test module. → `test_app::assets_with(source)` and `test_app::bare()`.
- [ ] `verifier/tests/binary.rs:16-80` starts the binary 5 times, each of which reads every workspace package. → One success and one failure, on a store of the lane mode.
- [ ] `import/src/zero_hour/tests.rs:76-85,963,994,1018-1024` runs full imports to disk several times. → One fixture import in a `OnceLock`.
- [ ] `net/src/local/local_server/tests.rs:45-48,118-123` drives a real server thread on wall-clock time with 2 ms sleeps (0.80 s alone). → Time stepped by hand, or the threading check split from the save, load, report and replay flow.
- [ ] `timeline_switch_plugin/tests.rs:36-102` and `correction/tests.rs:50-75` copy the `Ease` and `Diffable` code of `correction_component!` (`fixtures/protocol.rs:23-50`). → Use the macro. `net-harness/tests/fog.rs:35-39` writes a unit search that `MatchUnits` has. `Hero` and `hero()` are written in both `verifier/tests/session_logs.rs:115-129` and `net-harness/tests/prototype.rs:50-63`. → One home each. `server_socket/tests.rs:90-93` sleeps 400 ms. → A shorter `TestLink`.
- [ ] `use_action_data.rs:74` asserts that `needs()` returns the array that it is written as. → Delete it.
- [ ] `client/src/report.rs:62-71` reads the key itself each frame, with 9 parameters. → `.run_if(InputMap::on(Act::Report))`, as `shop_panel.rs:62` does.

### 19. Names, docs and small code (actions, abilities, script, units, declarations, interaction, navigation, geometry, math, sim)

Files: about 70 sites of a bare `slot: u8` (`ActionSlots`, `ActionProgress::order`, `SlotsMut::fill`, `AbilitiesEffect::Learn`), `capabilities/core/declarations/{Cargo.toml, src/callback_table/mod.rs, src/spawn_parts.rs}`, `capabilities/core/script/src/scripts/pool/`, `capabilities/core/actions/src/targets.rs`, `capabilities/core/units/src/units.rs`, `capabilities/interaction/src/interaction_rules.rs`, `capabilities/navigation/src/{progress.rs, steering/mod.rs}`, `sim/geometry/src/grid_search.rs`, `sim/math/src/{wide_division/portable.rs, vec3/mod.rs}`, `session/host/src/server/server_phase.rs`, `apps/server/src/opening/mod.rs`, `base/store/src/append_writer/mod.rs`.

- [ ] About 70 parameters are a bare `slot: u8`, against 02-engine-core.md Names ("a bare `slot` names neither"). `callback_table/mod.rs:9` uses `slot` for a place in a registry. → An `ActionSlotIndex` newtype, and `place`.
- [ ] "Pool" means a script budget (`scripts/pool/mod.rs:10`, `ScriptPools`, `Role::pools()`) and a stat pool (`PoolId`, `PoolCost`, `PoolPurse`, `Pools`). → The script side says "budget".
- [ ] `targets.rs:220` `Targets::body` builds a `LivingUnit`. → `living_unit`. `units.rs:243-247` joins the doc of `kinds` to that of `bodies`. → Move it to `kinds` (`:263`).
- [ ] `declarations/src/spawn_parts.rs:2,18` wraps the scratch in a `Mutex` that it never locks, only to be `Sync`, with two `expect`s for a poisoning that cannot happen. → `bevy_platform::cell::SyncCell`, as Bevy's system parameters and `net-replication`'s `SpareBuffers` hold, with `bevy_platform` named in `declarations`' manifest.
- [ ] `interaction_rules.rs:61,76` walks `cx.packages()` twice. → One walk.
- [ ] `progress.rs:106-120` has `restart` and `passed_waypoint` with identical bodies, and `:53-66` has `stuck_after` and `ask_after`, which differ in one constant. `steering/mod.rs:445` derives `cell × YIELD_WINDOW` beside `reach` (`:154`). → One function each.
- [ ] `grid_search.rs:9` builds and sorts its table of 51,433 steps in the first `GridSearch::nearest`, a script call in a tick. → Force it at the navigation install.
- [ ] `wide_division/portable.rs:12-21` says one rounding, but the high half can pass 2⁵³, which makes three (the bound still holds). → Split at 50 bits, as `root::Near::of` does. `vec3/mod.rs:175-183,203-214` repeats one block. → `fn wide_axes`.
- [ ] `server_phase.rs:28` `ServerPhase::now` reads `NextState` internals by hand. `server/src/opening/mod.rs:60-75` checks a restore's picks in the server binary, apart from `PendingRestore::check` in net. → Read through Bevy's API, and move the checks beside `PendingRestore::check`.
- [ ] `append_writer/mod.rs:273`: the drop waits for the worker only because `_worker` drops after `Drop::drop`, and `close()` and the publication depend on that order with no comment. → Join explicitly in `drop`.

### 20. Book builders beside their types (areas, combat, interaction, items, navigation, production, progression, projectiles, vision)

Files: `capabilities/production/src/book_builders.rs` and the type files of its 13 types, `capabilities/combat/src/combat_books.rs`, `capabilities/navigation/src/navigation_books.rs`, `capabilities/{areas/src/area_books.rs, interaction/src/interaction_books.rs, items/src/item_books.rs, progression/src/progression_books.rs, projectiles/src/projectile_books.rs, vision/src/vision_books/}`, and the type files each one reaches into.

- [ ] Each capability's `build` and `book` methods live in one load file, apart from their types: `book_builders.rs:184-576` holds the impls of 13 types, `combat_books.rs:115-242` of 6, and `navigation_books.rs:21-153` a second and a third `impl Navigation`. The guide puts one major struct per file with all its impls. → Each method moves to its type's file. A load file keeps only what belongs to no one type, as the capability's walk of its sections. No behavior changes.
