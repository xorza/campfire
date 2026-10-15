# Review: the whole workspace

Scope: every crate under `source/crates` and `source/checks`, production code only. Paths are relative to `source/crates/` unless they start with `source/` or `docs/`. The review found about 30 bugs and about 150 structural findings. The production module graph has four real cycles; no crate depends on a higher layer.

Each root cause below names the mechanism that lets the defect happen, the structural change that removes the mechanism, and the guard that makes the defect class loud the next time. A fix at one site, with no change to the mechanism, is not in this plan.

## 0. Decisions

The user decided each question on 2026-10-15. Each plan item that a decision shapes names it.

- **D1. Where this work goes:** the whole review is taken first, as a hardening section of `PLAN.md` before Zero Hour's batch (U6 to U8).
- **D2. What a predicting client pays:** the server alone pays, refunds and despawns. A predicting client shows the server's values. The costs, refunds and site despawns of production install through `server`, as attacks and casts do, and design 14 D12 names them.
- **D3. `package` and the deterministic core:** `package` splits. A core crate checks and decodes packages from bytes, and a crate outside the core reads directories through `store`. Design 02's rule, that no core crate depends on `store`, stays as written.
- **D4. The home of the client's formats:** a new crate at the session layer holds the client's presentation formats and `HeightGrid`. Only the import and the client depend on it.
- **D5. The Zero Hour check against design 12 D7:** D7 stays the target. The plan holds the fingerprint comparison and the parity suite as open steps for when the oracle comes, and design 02's dependency list follows the code.

## 1. Proposal

### RC1. A restore check covers its own type's shape, not what later code computes on it

- **Mechanism.** `SimComponent::check` is a free `bool` function for each type. The arithmetic on state uses panicking operators and `expect`. A rule between two parts (a forced move needs a walker, an inventory is as long as its carrier's slots) lives in one of the two types' checks or nowhere. The drawn-values test (`test/harness/tests/runner/proving_match.rs:413`) passes over a state type that no fixture entity holds (`scramble_component` returns `false`, and the loop continues), and it steps 5 ticks with no actions, so the paths that compute on a restored value never run.
- **Structural change.**
  1. Code that computes on state is total over the values the check accepts. It computes in the wide domain of design 09, one `i128` step rounded once, rather than through a conversion that can fail. A span of ticks scales by a fraction as `u64 × Num bits` in `u128`, not through `Num::from_int(…).expect`. A step that passes the world's bound saturates at the bound.
  2. A rule between parts is a declaration. Each kind declares every part an entity of it may carry, the state markers that come and go among them (`Dead`, and `ForcedMove` on walkers alone). One restore walk refuses an entity that carries a part its kind does not declare. No type's check repeats the rule.
  3. A value that a book fixes is checked against the book or is not stored. An inventory's length is its carrier's slot count, and a spawn point lies within the map's `Bounds`.
  4. The checked terms are a type. `SessionRules::check` returns `CheckedTerms`, which carries the decoded `Visibility` and the `InputRules`. Code after the check takes `CheckedTerms`, so no code can read restored terms before the check.
- **Guard.** The drawn-values test fails when a state type has no holder. Its fixture holds every state type (a restore fixture built for the purpose, or the 3v3 at a tick that holds them all), and after each restore it plays a scripted window that fires each effect kind of its mode once.

### RC2. Untrusted file data reaches code that trusts it

- **Mechanism.** `PackageDir::read_bytes(path, None)` reads "as many as the file holds", so a read can leave its bound out. The book builder indexes the content maps with `[]` and trusts `session/package`'s load check to have refused a bad name first. Each binary format computes its sizes with its own unchecked arithmetic.
- **Structural change.**
  1. A package read takes a `ReadBound`, exact or at most, with no "whole file" case.
  2. The book builder is the check, as design 02 says. The builder reaches content only through name tables whose lookup returns its rule's case. `load_check.rs` keeps only the rules that span books, maps the builder's cases, and builds no table of its own. This also removes RC3's duplicated tag, type and name rules.
  3. Each binary format decodes its sizes through one checked function of its own.
- **Guard.** Every truncation and every flip of each package binary format (the index, KTX2, terrain, heights) is refused and never panics, as `session_log::tests::every_truncation_and_every_flip_of_a_log_file_is_refused` holds for the session log.

### RC3. One fact has two homes

- **Mechanism.** A format, a rule or a derived fact is written where it is used, with no owner both sides must use.
- **Structural change, by kind.**
  - **A format** is one type that both reads and writes it, in the crate that owns the format. The import writes `MapData` and the units file through their own types, and the KTX2 writer sits beside its reader.
  - **A rule** lives in the builder or the capability that holds the data. Navigation owns the map walk check, and the builder owns the load's name rules (RC2).
  - **A derived fact** is derived once at its boundary and passed as a type: `CheckedTerms` (RC1), the visibility, the nested `InputRules`.
  - **A list** comes from a walk of the declarations. The data filters come from `Release::state_types`, the engine tags from the install order, and the book parts from the modules.
  - **Duplicated code** goes to one function: the version parser, the tagged file, the rounding helper, `create_dir_all`, the snapshot check, the session opening, the lobby's offer and refuse.
- **Guard.** Each format has a round-trip test in its owning crate: the writer's output reads back equal. Each list made by a walk has the declaration audit as its check.

### RC4. Passes scan where they should visit what changed

- **Mechanism.** Systems query every unit and test each one, and no shared structure holds "work due at tick t".
- **Structural change.** Event-driven passes use `Changed<…>` filters, and a full walk runs only when a book or the static set changes. Timed work goes through one `DueRun`, a run kept sorted by due tick, in `sim`. It serves the think pass, the charges, the toggles and any later timer. It is a cache that each restore rebuilds, outside the state hash. A change to the static bodies patches the rows and the regions it touches.
- **Guard.** Each item names its bench (AGENTS.md, Benches), which the next optimization session runs. No test bounds a pass's cost.

### RC5. State and parts enter outside the declarations

- **Mechanism.** `InstallCx` exposes `pub world` and `pub schedule` (`declare/src/declare/install_cx.rs:16`), so an install inserts resources and hooks (`units/src/units/mod.rs:234`, `combat/src/lib.rs:169`, `navigation/src/lib.rs:193`, `stats/src/lib.rs:242`). A spawn's extra bundle is optional, so parts come after the spawn by command.
- **Structural change.** `InstallCx` keeps its world and schedule private and gives an install only system installs: `systems`, `server`, `scripted` (RC8). A resource, a hook and a part come from declarations, which the `Installer` walks. A hand insert in an install no longer compiles. Every part a spawn needs goes in the spawn.
- **Guard.** Besides the compile error, a harness test over the scripted matches records each unit's archetype at its spawn. After the spawn, a unit may change archetype only by its kind's declared state markers.

### RC6. The published surface is wider than its callers

- **Mechanism.** The world-trait rule test checks trait names, not where they are used. `codec` and `schedule` have no `internals` feature, and `store` does not put its test seams behind its feature.
- **Structural change.** Cross-crate methods move to `<Module>World`. Test seams go behind `internals` in each base crate. Fields become private with accessors, and dead shipped code goes behind `internals` or is deleted.
- **Guard.** The world-trait rule test fails on a `*WorldCrate` import outside its crate.

### RC7. Wrapper modules hide the cycle test

- **Mechanism.** `no_crates_modules_import_each_other_in_a_cycle` (`base/common/src/tests.rs:1025`) reads top modules only. `units`, `declare` and `players` each put the whole crate under one child, so the test sees one module there.
- **Structural change.** The test checks the sibling modules under every parent of the module tree, not only the top. The wrappers go, and each cycle the test then names is cut at its edge against the layering.
- **Guard.** The test itself, which now sees every level.

### RC8. The role and prediction policy is decided case by case

- **Mechanism.** `InstallCx::role()` lets each install branch on the role freely. `Role::scripts()` serves as a server gate for systems that call no script. No written rule says what a predicting client pays (D2). The client infers "avatar" from `Owner`.
- **Structural change.** `InstallCx` gives one install method for each role-shaped need, and no `role()`:
  - `server` for systems that compute the server's own state;
  - `scripted` for systems that call scripts;
  - `systems` for systems that every match runs.

  A role fact that a part depends on is fixed at install, as `SeenBy` should be. The prediction rule of D2 joins design 14 D12, and each cost, refund and despawn installs by it. The client and the harness ask `PlayerUnits` who a player commands, the one rule the server uses.
- **Guard.** The test of design 14 D12 reads `ServerOnly`, and it covers the costs that D2 gives to the server.

### RC9. Results depend on archetype order

- **Mechanism.** A query walks archetype order, which a restore or one insert changes. `sim::Ordered` exists for systems that need stable-id order, and a system that pushes into ordered state without it differs after a restore (`combat/src/dying.rs:42-52`).
- **Structural change.** A system whose output order reaches state or a script walks through `Ordered`.
- **Guard.** A restore target that spawns its entities in reverse order. The resume test then compares each checkpoint's hashes against the run that did not restore.

### RC10. Sibling APIs drift apart

- **Mechanism.** No shared type holds an operation that several siblings need.
- **Structural change.** The `NameId` trait owns the id surface (`at`, `index`, `LIMIT`), and no id repeats it inherently. One generic callback table serves every registry keyed by an `enum_all` kind. A `Placed` body value carries a position and a shape into each reach and approach function. The store workers share one failure and lock-poison helper.
- **Guard.** None beyond the shared types.

## 2. Plan

Batches go most impactful first. A batch that needs another batch or a decision names it. Each batch adds its guard first: the guard fails on the bugs, then each fix makes it pass, so the batch proves what it fixed. Each batch ends with `./drive.py check` passing, and it says whether the goldens move.

### Batch 1. Restore safety (RC1)

Files: `test/harness/tests/runner/proving_match.rs`, `sim/sim/src/state_registry/{mod,internals}.rs`, `capabilities/core/declare/src/**`, `capabilities/core/units/src/units/{forced_move,spawn_point}.rs`, `capabilities/core/actions/src/action_slots/{mod,slot_times}.rs`, `capabilities/mode/src/choices.rs`, `capabilities/items/src/inventory.rs`, `capabilities/navigation/src/{static_bodies,walking}.rs`, `session/runner/src/session_rules/mod.rs`, `apps/server/src/{main,opening/mod}.rs`, `network/net/src/sim_{server,client}/mod.rs`. Goldens: none move, as the wide-domain arithmetic rounds once, as the `Num` arithmetic did.


### Batch 2. Order and roles (RC9, RC8)

Files: `sim/sim/src/{sim_world,state_registry/mod}.rs`, `test/harness/tests/runner/resume.rs`, `capabilities/combat/src/{dying,attacks}.rs`, `capabilities/core/declare/src/declare/{install_cx,role}.rs`, every module's `install`, `capabilities/abilities/src/lib.rs`, `capabilities/items/src/lib.rs`, `capabilities/vision/src/lib.rs`, `sim/sim/src/{capability,sim_rng}.rs`, `capabilities/src/release/*`, `capabilities/core/script/src/scripts/script_call.rs`, `capabilities/production/src/{trains,lib,production_orders}.rs`, `docs/design/14-replication-audiences.md`. Goldens: they move for the RNG key, and maybe for the order of deaths.


### Batch 3. Package crates (D3, D4)

Goes before the batches that edit `package`, so each fix lands in its final crate. Files: `session/package/**`, `session/runner/src/**`, `session/verifier/src/**`, `capabilities/mode/src/height_grid.rs`, the `Cargo.toml` of every crate that depends on `package`, `docs/design/02-engine-core.md`. Goldens: none move. The packages' fingerprints stay byte-equal.


### Batch 4. Untrusted files (RC2)

Files: `session/package/src/{package_dir/mod.rs,client_data/texture_levels.rs,mode_packages/{load_check,name_tables}.rs}`, `capabilities/src/books/*`, `capabilities/core/actions/src/{effect_list_book,slot_kinds}.rs`, `capabilities/items/src/{shop,inventory}.rs`, `capabilities/core/geometry/src/{polygon/mod,bounds}.rs`. Goldens: none move.


### Batch 5. Store and network bugs

Files: `base/store/src/{data_dir/mod.rs,platform/owner_only.rs,durable_file/*,stream_writer/mod.rs,exchange/mod.rs,append_writer/mod.rs}`, `network/quic/src/{link_end,quic_plugin,endpoint_state,port_mapper/mod}.rs`, `network/net/src/sim_server/data_filter.rs`. Goldens: none move.

- [ ] `quic/src/link_end.rs:252-259`: a `LinkEnd` case for an app close that the handshake hid (`Transport { code: 12 }` before 1-RTT, RFC 9000 §10.2.3), mapped by `LinkLossCause`. Test: a close before the handshake ends reports the new case. This should remove the issue log's flaky `quic_endpoint` test item.
- [ ] `quic/src/quic_plugin.rs:54`: `try_insert`, as `unlink` has.
- [ ] `quic/src/port_mapper/mod.rs:201-203`: the PCP lifetime is clamped to `retry`.
- [ ] `net/src/sim_server/data_filter.rs:236-260`: the remove observers build through `DataFilter::of`.
- [ ] Store workers (RC10): one failure and lock-poison helper serves `AppendWriter`, `StreamWriter`, `LatestWriter` and `Exchange`. With it, `stream_writer/mod.rs` gets `StreamError { Write, Flush }`, a `take_failure` while it runs and a poison-tolerant drop, and `exchange/mod.rs:86-91` resumes a dead worker's panic.
- [ ] `store/src/durable_file/*`: a `DurableDir` with its own error enum. `data_dir/mod.rs:30` uses it, and `OwnerOnly::create_dir_all` (`platform/owner_only.rs:37`) goes.

### Batch 6. Client and harness

Files: `apps/client/src/{hud/mod.rs,hud/gauge.rs,view/look.rs,view/mod.rs,main.rs}`, `test/harness/src/{in_process_match,moba_3v3}/mod.rs`, `source/checks/lan-check/src/{main,error}.rs`. Goldens: none move.

- [ ] Who a player commands: `view/look.rs:90` and `view/mod.rs:136,326,339` take `Has<Avatar>`, and a model drawing's height comes from its bounds. `harness/src/in_process_match/mod.rs:725` asks `HeldPlayerUnits::avatar`.
- [ ] `hud/mod.rs:289`: the avatar's gauge rows are rebuilt when `Changed<ActionSlots>` changes the slot layout.
- [ ] `hud/gauge.rs:94`: the row centres on its own count of the ability kind's slots.
- [ ] `view/mod.rs:471` (`lean`): test `Look` first.
- [ ] `client/src/main.rs:9`, `lan-check/src/main.rs:17-20`, `error.rs:19`: the docs name the real keys and `./drive.py lan`.
- [ ] `harness/src/moba_3v3/mod.rs:425-460`: the hero lookups go through `MatchUnits`, and the inventory's first slot comes from `first_of`.

### Batch 7. The module graph (RC7)

Files: `base/common/src/tests.rs`, `capabilities/core/{units,declare,players}/src/**`, `capabilities/core/script/src/scripts/{view/*,core_api.rs}`, `network/net/src/sim_server/{session_dir,checkpoints,sim_server_world}/*`, `capabilities/orders/src/{lib,unit_order}.rs`. Goldens: none move.

- [ ] Guard: the cycle test checks the siblings under every parent of the module tree.
- [ ] The wrappers go: `units/src/lib.rs:14`, `declare/src/lib.rs:10`, `players/src/lib.rs:6`.
- [ ] `units`: cut `unit_state_book.rs:11` (the column takes the book as the match begins), `new_unit_states/mod.rs:6` (move `StateWrite`) and `unit_state_access.rs:5` (`of_new` takes the id and the type). `tag_effect` becomes a child of `tag_book`.
- [ ] `script/src/scripts/view/{mod.rs:309,unit.rs:51,filter.rs:97}`: the view's script bindings move beside `core_api.rs`.
- [ ] `net/src/sim_server/session_dir/mod.rs:218-255`: one replay in `sim_server` serves restore and abort, resuming from the snapshot.
- [ ] `net/src/sim_server/checkpoints/mod.rs:26`: `Checkpoints` returns its decision, and the world trait acts on it.
- [ ] `orders/src/lib.rs:137-149`: `apply_order` is inlined into `UnitOrder`'s `Effect` impl.
- [ ] Any further cycle the guard names.

### Batch 8. Declarations own install and spawn (RC5)

Needs Batches 2 and 7. Files: `capabilities/core/declare/src/**`, `capabilities/src/release/installer.rs`, the `install` of `units`, `combat`, `navigation`, `stats`, `mode`, `capabilities/orders/src/thinking.rs`, `capabilities/production/src/{construction,gather_loop}/mod.rs`, `capabilities/core/deliveries/src/delivery_spawner.rs`, `capabilities/progression/src/{progression_effect,experience}.rs`, `capabilities/core/units/src/units/{unit_type_book,spawner}.rs`, `capabilities/src/books/*`. Goldens: they may move where a part now comes at the spawn. The batch says which.

- [ ] Guard: a unit changes archetype after its spawn only by its kind's declared state markers, in the scripted matches.
- [ ] `InstallCx` keeps `world` and `schedule` private. The resources and hooks of `units/src/units/mod.rs:234`, `combat/src/lib.rs:169`, `navigation/src/lib.rs:193` and `stats/src/lib.rs:242` become declarations. This removes the issue log's **Declare** item.
- [ ] `combat/src/lib.rs:170-178`, `mode/src/lib.rs:281,285`: `Deaths` is declared, and the damage and heal weighers are a declared callback registry that mode fills.
- [ ] `stats/src/lib.rs:278-302`, `mode/src/mode_units.rs:22-44`, `unit_type_setup.rs:10`: a stats `ByType` passive book and a `SpawnHooks` callback. Delete `Stats::grant`, `ModeUnits::apply_passive` and `UnitTypeSetup.passive`.
- [ ] `mode/src/mode_map.rs:227-309`: navigation owns the map walk check, and its eight types go back to `pub(crate)`.
- [ ] `orders/src/thinking.rs:133-136`, `production/src/construction/mod.rs:193,215`, `gather_loop/mod.rs:366`: `NextThink`, `Site` and `StatusTags` come with the spawn.
- [ ] `deliveries/src/delivery_spawner.rs:54-59`: a delivery spawns through the one core spawn of `units::Spawner`.
- [ ] `progression/src/progression_effect.rs:60-75`, `experience.rs:20-30,83`: progression asks stats for a level change through a contract below both, as design 04 says (stats owns the level, progression decides when it changes). `TrackXp.level: Option<Level>` goes.
- [ ] `capabilities/src/books/{book_builder.rs:229-321,book_parts.rs,unit_type_data.rs}`, `areas/src/lib.rs:115`, `projectiles/src/lib.rs:113`, `production/src/lib.rs:188-202`: each module's book declaration carries its build function, `into_books` walks the declarations, and the engine tags are numbered by the walk.
- [ ] `units/src/units/unit_type_book.rs:37,42,153,210`: `take()` the origins and the states, as `staged_tags` does.
- [ ] `values/src/relations.rs:94-100`: `seen_targets.rs:39` reads `VisionGroups`.
- [ ] `mode/src/map_data.rs:112`: delete the unread `MarkerData.events`.

### Batch 9. One home for each session fact (RC3)

Needs Batch 1 (`CheckedTerms`). Files: `session/{protocol,runner,package}/src/**`, `apps/server/src/{main,opening/mod}.rs`, `network/net/src/local/local_server/mod.rs`, `network/net/src/sim_server/{mod,lobby/mod,lobby/door,data_filter}.rs`, `sim/math/src/{num/mod,direction/mod}.rs`. Goldens: none move. The session-id hash stays byte-equal.

- [ ] `runner/src/input_rules.rs:6`: `InputRules` moves to protocol, nested in `SessionTerms`.
- [ ] `apps/server/src/opening/mod.rs:110-158`, `main.rs:171-228`, `net/src/local/local_server/mod.rs:51,134-169`: one net constructor opens a lobby for both servers, from `ServerSetup` and its clock. `Started` goes.
- [ ] `runner/src/session/mod.rs:306-336`, `replay/mod.rs:96-114`: one snapshot check that returns `SnapshotCheckError`.
- [ ] `net/src/sim_server/lobby/mod.rs:168-239`, `door.rs:59-115`: one `Offering`.
- [ ] `net/src/sim_server/mod.rs:269-325,386-458`: one private step serves `start_match` and `restore_match`.
- [ ] `net/src/sim_server/data_filter.rs:14-29,89-125`: the filters come from the `Release::state_types` walk, and `net` drops its dependencies on `items`, `navigation`, `production` and `stats`.
- [ ] Protocol signing: a `Signer { secp, key }` and a verification context with its scratch message, used by `checkpoint`, `session_result`, `server_input`, `receipt`, `input_chain`, `connect` and `session_log`. `ServerSigner` goes.
- [ ] `runner/src/slot_rules/mod.rs:20-50`, `mode::Leaver`, `protocol::AfterLeave`: one type in `common`.
- [ ] `package/src/files/version.rs`, `protocol/src/engine_release/mod.rs`: one version type in `common`.
- [ ] `protocol/src/receipt/mod.rs:86-105`, `session_private/mod.rs:29-48`: one tagged-file helper and error.
- [ ] `protocol/src/session_log/mod.rs`: the durable-head tracking becomes its own type, and the file and journal codecs become child modules.
- [ ] `package/src/package.rs:19,72`: `fingerprint` is read through the files.
- [ ] `sim/math/src/num/mod.rs:171`, `direction/mod.rs:69,133`: `Rounding::shift_right`.
- [ ] `store/src/platform/stop_signals.rs:22`: `ExitStatus::Interrupted`, pinned to `StopSignals::EXIT_CODE` by a check.

### Batch 10. One home for each format (RC3)

Needs Batch 3. Files: `apps/import/src/{zero_hour,texture}/*`, the client formats' crate of Batch 3, `capabilities/mode/src/map_data.rs`, `apps/client/src/{view/client_data,view/ground_heights,args,connection}*`. Goldens: none move. The imported packages stay byte-equal, or the batch says why not.

- [ ] Guard: a round-trip test for each format the import writes.
- [ ] `import/src/zero_hour/map_import.rs:69`, `unit_types.rs:20`: `MapData`, `PlacedUnitData`, `MarkerData` and the units file type derive `Serialize`, and the import writes them.
- [ ] `import/src/texture/ktx2_image.rs:14`: one KTX2 writer beside the reader, which the `internals` fixture uses too.
- [ ] `import/src/zero_hour/mod.rs:232`, `unit_types.rs:34`: one `ImportName` table over every object, with one clash error.
- [ ] `import/src/zero_hour/{chunk_reader,w3d_fields}.rs`: one little-endian cursor. `w3d_file.rs:51` and `w3d_mesh.rs:185` return the iterator's error.
- [ ] `import/src/zero_hour/map_object.rs:218`, `client/src/orders.rs:221`: one exact `Num::try_from(f32)` in `campfire-math`.
- [ ] Client data: one plugin owns `ClientData`, `ModePackages::mode()` replaces the five `packages().next().expect` sites, and places are `PackageIndex`. `GroundHeights` owns the grid, with its extremes computed once.
- [ ] `client/src/args/mod.rs:22,40`: `Server::Local { bots, data }`.

### Batch 11. Change-driven passes (RC4)

Files: `sim/sim/src/` (the new `DueRun`), `capabilities/navigation/src/**`, `capabilities/stats/src/held_pass.rs`, `capabilities/core/actions/src/holds.rs`, `capabilities/orders/src/thinking.rs`, `capabilities/production/src/gather_loop/mod.rs`, `capabilities/projectiles/src/flying.rs`, `capabilities/abilities/src/toggles.rs`, `session/{runner,protocol}/src/**`, `network/quic/src/admission.rs`, `capabilities/core/geometry/src/body_box/mod.rs`. Goldens: none move. A pass visits fewer units with the same result.

- [ ] `DueRun` in `sim`, a cache that each restore rebuilds. `orders/src/thinking.rs:74-93`, `actions/src/holds.rs:22-36` and `abilities/src/toggles.rs:20-64` use it.
- [ ] `navigation/src/static_tracking.rs:46-58`, `body_index/mod.rs:206-293`, `regions/mod.rs:257-417`: added and removed bodies only, patched rows, and dirty region chunks.
- [ ] `navigation/src/routing.rs:325-403`, `route_asks.rs:127-152`: `Changed<Destination>`, every walker only on a static change, and a list of waiting asks.
- [ ] `navigation/src/steering.rs:80`, `walking.rs:320-327`, `navigation_column.rs:78-94`: the walking grid built at the first stuck walker, `keep_in_bounds` over `Changed<Position>`, and `room_for` near the box.
- [ ] `stats/src/held_pass.rs:105-208`: re-derive the holds on a change of owner, tags or `PlayerModifiers`.
- [ ] `actions/src/holds.rs:45-106`: `want_passives` over `Changed` filters and `ReadyWaits`.
- [ ] `production/src/gather_loop/mod.rs:176-190,331`, `projectiles/src/flying.rs:26-35`: waiting workers by node, `Changed<Node>`, and homing projectiles by target.
- [ ] `protocol/src/server_seeds.rs:24-33`, `runner/src/session/mod.rs:107-117,217-224`: test `segment <= last` and derive the seed once, and one restore target for each replay.
- [ ] `quic/src/admission.rs:18-84`: a map and an expiry queue.
- [ ] `geometry/src/body_box/mod.rs:336-343,533`: pass the `Edges` and the probe already made.
- [ ] Waits for the user's optimization session: each item's bench.

### Batch 12. The published surface (RC6)

Needs Batch 7. Files: the `*_world.rs` of `players`, `script`, `units` and `stats`, `network/net/src/sim_server/{sim_server_world/mod,player_link}.rs`, `base/{codec,schedule,store,log}`, `session/{package,runner}/src/**`, `base/common/src/tests.rs`. Goldens: none move.

- [ ] Guard: the world-trait rule test fails on a `*WorldCrate` import outside its crate.
- [ ] `PlayersWorldCrate`, `ScriptsWorldCrate`, `UnitsWorldCrate`, `StatsWorldCrate`: their cross-crate methods move to `*World`.
- [ ] `net/src/sim_server/sim_server_world/mod.rs:24-62`: `SimServerWorldInternals` and `SimServerWorldCrate`, and `PlayerLink` behind `internals`.
- [ ] `codec` and `schedule` gain `internals`. `binary.rs:57-91`, `drop_option.rs`, `error.rs:37`, `strict_schedule.rs:24`, `append_file.rs:5` and the generic `start`s go behind it.
- [ ] `log/Cargo.toml:18-25`: the `start`-only dependencies become optional. `log/src/log_line/mod.rs:17,41`: codec owns the decode, and `LogLine` publishes no `serde_json` type.
- [ ] Dead shipped code in `package` and `runner` goes behind `internals`. `ModePackages::script_book()` and `Backends` are deleted.
- [ ] Private fields with methods: `script_api`, `script_batch`, `Deliveries`, `StatsCall.handles`, `Applier`, `Timers`, `Choices`, `StatsData`, `EntityKinds`, and `Frame`'s `effects`.
- [ ] The narrowest visibility for `Unit::new`, `record`, `Halves`, `Polygon::holds`, `Approach`, `Navigation::walker`, the `push`es of `StatsColumn` and `NavigationColumn`, `snapshot_hash`, `restore_target`, `PackageView`, `production::Held`, the `RowReads` aliases, `TagProperty::ALL` and `PlayersData::DEFAULT`.

### Batch 13. Names resolved at the load

Files: `capabilities/stats/src/{param_book,modifier_spec}.rs`, `capabilities/mode/src/{game_map,marker,marker_tag,marker_spec,mode_map}.rs`, `capabilities/core/{script,actions,values}/src/**`, `session/protocol/src/**`, `network/net/src/events/*`, `network/quic/src/{port_mapper/mod,error}.rs`. Goldens: none move.

- [ ] `stats/src/param_book.rs:73-77`: each (modifier, action) param place resolves at the load.
- [ ] `mode/src/{game_map.rs:48-55,marker.rs:32,marker_tag.rs:118,marker_spec.rs:6}`: interned marker tags, and `DeclaredName` names.
- [ ] A `PackageId` with `MODE` replaces the raw `u16` in `call_start.rs:20`, `frame.rs:64`, `action.rs:14`, `ActionBook::load` and `ActionNames`.
- [ ] A typed main key replaces `[u8; 32]` in `protocol` (`delegation`, `controller`, `session_log`, `log_carry`). `SlotPlan` converts from its byte.
- [ ] `net/src/events/{save,receipt,join}_refused`: typed fields, as `LinkLost` has. `quic/src/error.rs:22`: its own variant for a gateway with no IPv4 address.

### Batch 14. Sibling APIs (RC10)

Files: `capabilities/core/{values,geometry,actions,deliveries,players,units,script}`, `capabilities/{stats,combat,abilities}`, `network/{net,quic}`, `apps/{client,import,server}`, `test/harness`. Goldens: none move.

- [ ] Ids: `NameId` owns `at`, `index` and `LIMIT`, and every id in `values` has one shape. `unit_type_core.rs:21-24` reads the ids' limits. `TeamSet` and `TagSet` share one surface.
- [ ] Geometry: a `Placed` body in `metric.rs:59-121`, `shape/mod.rs:59` and `body_box/mod.rs:274`. `comes_within` becomes `comes_closer`. A `Closure` enum serves `body_box/mod.rs:572-671`, one span helper serves `grid/mod.rs:147-224`, a checked `Radius` sits in `shape/mod.rs:16-19`, and `RowDirectory` moves into `geometry`.
- [ ] One generic callback table: `deliverers.rs:39`, `effect_queues.rs:28`, `cast_spends.rs:25`, `spawn_hooks.rs:24`. `view_names.rs:22` uses `TypeSlots`.
- [ ] `deliveries/src/deliverers.rs:11`: `Deliver` takes a `Launcher`, and `Launcher::of` builds it.
- [ ] Stats: one shape for the meters and the ids, `held_handle` registers itself as `applied_handle` does, and `Lifetime` gets two constructors.
- [ ] One field name, `unit`, for the unit an effect acts on (`combat_effect.rs`, `abilities_effect.rs`, `combat_event.rs`).
- [ ] `PlayerRowsMut` gains `of_slot`. `PlayerResources::affords` replaces `Purse`'s copy, and the `Payer` doc is corrected.
- [ ] `script/src/scripts/error.rs:26`: one case that carries a capability's own refusal type. The validators move to the argument code.
- [ ] `units` `SyncTo`: one dispatch. `net` `FaultSource` and `PaceSpeed::place` go. `quic`: one constructor for the server endpoint with its `AddressLimit`, and one for the minimum-MTU link.
- [ ] Flag parameters become enums, and wide signatures become parameter structs with the world first. The full list is in Section 3.
- [ ] Small guide items, also in Section 3.

### Batch 15. Design and code that disagree

D5 shapes it. Files: `docs/design/02-engine-core.md`, `docs/design/12-zero-hour.md`, `capabilities/src/{lib.rs,release/tests/mod.rs}`, `capabilities/areas/src/lib.rs`, `base/common/src/tests.rs`, `source/checks/*`.

- [ ] Guard: every crate has a layer, and a test holds the rule "no two crates of one layer depend on each other" for all of them. The test also checks "one above the highest dependency", which today places `vision` at 10 where the rule gives 7.
- [ ] D5: design 12 D7 stays. `PLAN.md` holds the fingerprint comparison and the parity suite as open steps, and design 02's dependency list of `zero-hour-check` follows the code.
- [ ] `capabilities/src/lib.rs:7-13`, `areas/src/lib.rs:43`: the docs follow `LAYERS` and `NEEDS`.
- [ ] `checks/zero-hour-check/src/main.rs`, `lan-check/src/run_dir.rs`: one run-root helper, in a home the design names.

## 3. Lists that Batch 14 takes

- **Flag parameters:** `HudPalette::fill`, `View::rest`, `Shape::of`, `Look::pose`, `CameraRig::zoom_key`, `turn_key`, `shop_panel::dimmed`, `Orders::slot_action`, `MapFile::texture_class`, `GlbBuilder::accessor`, `InProcessMatch::lose_server_packets`, `ViewRows::column_mut`, `SupplyCostBook::unit`, `SimClient.local`, `out_of_play`.
- **Wide signatures:** `ActionBook::load`, `EffectListBook::queue`, `DeliverySpawner::spawn`, `SessionRules::terms`, `resume`/`resume_match`/`load_save` (two argument orders), `install_mode` and `books` (world second), `write_slot`, `terrain_image::copy`, `model_writer::convert`, `GlbBuilder::accessor`, lan-check `verify`, `Opening::find`.
- **Guide items:**
  - `names.rs:95` (`.ok()`), `spawn_match` (`impl IntoIterator`), `pending_calls.rs:29` (`Extend`), `api_builder.rs:184` (`assert!`).
  - `entity_parts.rs:32` (`Arc` and `PhantomData`), `actions_api.rs:34-40` (one effect key list), `effect_list_book.rs:205` (a list-hook enum), `unit_type_data.rs:90-178` (one table for `FIELDS` and `given`).
  - `book_builder.rs:436-441,548-552`, `sim_world_builder.rs:115-135`, `abilities_api.rs:131-136`.
  - The import's `as` casts, its missing `#[from]`, its functions on the wrong type, and the double decode in `class_texture`.
  - Misplaced docs: `units/mod.rs:276-279`, `tag_book.rs:88-91`, `stats_effect.rs:33`, `refresh.rs:34`. `refresh.rs:174,196-199`.
  - The server: `main.rs:12` (`#![allow]`), its repeated exit blocks, and `Args::of`.
  - Net: the `order_script` error cases, `bot_driver.rs:28`, the clone of `report`, `lose_link`'s `Signer`, and `LinkEnd::Refused`'s doc.
  - The harness: `duo`, `lane_mode` and `step`.
  - Math and log: the `UnitDivisor` choice and the `aarch64.rs` names, and `json_text.rs:12-15`.
  - Errors: `visibility/src/setting/error.rs:512-523`, `ModeError::RepeatedName`, `add_resource`'s `NoMode`.
  - `ModeMap::install_pathing`, `Frame::apply`'s growth assert, `Orders::apply_order`.
  - Two client allocations: `unit_models.rs:232` and `view/mod.rs:332`.
