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

- **Mechanism.** `InstallCx` exposes `pub world` and `pub schedule` (`declare/src/install_cx.rs:16`), so an install inserts resources and hooks (`units/src/units.rs:234`, `combat/src/lib.rs:169`, `navigation/src/lib.rs:193`, `stats/src/lib.rs:242`). A spawn's extra bundle is optional, so parts come after the spawn by command.
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

### Batch 8. Declarations own install and spawn (RC5)

Needs Batches 2 and 7. Files: `capabilities/core/declare/src/**`, `capabilities/src/release/installer.rs`, the `install` of `units`, `combat`, `navigation`, `stats`, `mode`, `capabilities/orders/src/thinking.rs`, `capabilities/production/src/{construction,gather_loop}/mod.rs`, `capabilities/core/deliveries/src/delivery_spawner.rs`, `capabilities/progression/src/{progression_effect,experience}.rs`, `capabilities/core/units/src/{unit_type_book,spawner}.rs`, `capabilities/src/books/*`. Goldens: they may move where a part now comes at the spawn. The batch says which.

- [ ] `capabilities/src/books/{book_builder.rs:229-321,book_parts.rs,unit_type_data.rs}`, `areas/src/lib.rs:115`, `projectiles/src/lib.rs:113`, `production/src/lib.rs:188-202`: each module's book declaration carries its build function, `into_books` walks the declarations, and the engine tags are numbered by the walk. Blocked: see `review-workspace_QUESTIONS.md`, Batch 8.
- [ ] `values/src/relations.rs:94-100`: `seen_targets.rs:39` reads `VisionGroups`. Blocked: see `review-workspace_QUESTIONS.md`, Batch 8.

### Batch 9. One home for each session fact (RC3)

Needs Batch 1 (`CheckedTerms`). Files: `session/{protocol,runner,package}/src/**`, `apps/server/src/{main,opening/mod}.rs`, `network/net/src/local/local_server/mod.rs`, `network/net/src/sim_server/{mod,lobby/mod,lobby/door,data_filter}.rs`, `sim/math/src/{num/mod,direction/mod}.rs`. Goldens: none move. The session-id hash stays byte-equal.

- [ ] Protocol signing: a `Signer { secp, key }` and a verification context with its scratch message, used by `checkpoint`, `session_result`, `server_input`, `receipt`, `input_chain`, `connect` and `session_log`. `ServerSigner` goes. Blocked: see `review-workspace_QUESTIONS.md`, Batch 9.

### Batch 10. One home for each format (RC3)

Needs Batch 3. Files: `apps/import/src/{zero_hour,texture}/*`, the client formats' crate of Batch 3, `capabilities/mode/src/map_data.rs`, `apps/client/src/{view/client_data,view/ground_heights,args,connection}*`. Goldens: none move. The imported packages stay byte-equal, or the batch says why not.

- [ ] Guard: a round-trip test of the units file the import writes; the map's and KTX2's are done. Blocked: see `review-workspace_QUESTIONS.md`, Batch 10.
- [ ] `import/src/zero_hour/unit_types.rs:20`: the units file type derives `Serialize`, and the import writes it. Blocked: see `review-workspace_QUESTIONS.md`, Batch 10.

### Batch 11. Change-driven passes (RC4)

Blocked: see `review-workspace_QUESTIONS.md`, Batch 11.

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

### Batch 13. Names resolved at the load

Files: `capabilities/stats/src/{param_book,modifier_spec}.rs`, `capabilities/mode/src/{game_map,marker,marker_tag,marker_spec,mode_map}.rs`, `capabilities/core/{script,actions,values}/src/**`, `session/protocol/src/**`, `network/net/src/events/*`, `network/quic/src/{port_mapper/mod,error}.rs`. Goldens: none move.

- [ ] `net/src/events/join_refused`: typed fields, as `LinkLost` has. Blocked: see `review-workspace_QUESTIONS.md`, Batch 13.

### Batch 14. Sibling APIs (RC10)

Files: `capabilities/core/{values,geometry,actions,deliveries,players,units,script}`, `capabilities/{stats,combat,abilities}`, `network/{net,quic}`, `apps/{client,import,server}`, `test/harness`. Goldens: none move.

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
