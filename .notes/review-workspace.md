# Review: the whole workspace

Scope: every crate under `source/`, except the test harness. Paths are relative to `source/`. Line numbers point to commit `22695ff7`.

## Proposal

The crate graph holds its layers, and a test holds them (02-engine-core.md, Modules). No module cycle crosses a layer: the only cycles are in `log` (a test reach-in) and `zero-hour-check` (its pins). The defects are inside the crates. Eleven causes explain most of them.

### 1. A rule the protocol states is not a rule the log holds

The verifier proves less than 05-protocol-spec.md's "A check proves" says. `check_result` accepts `aborted` for any match. A delegation has many spellings, so one log has many fingerprints. Nothing refuses one main key in two slots, or a rejoin that restarts a chain from the same root. The file bound counts payload bytes only, so a long session writes a journal that its own reader refuses. `LogCarry` is written and never read, so "a segment verifies from its checkpoint alone" is false.

Target: each rule of 05-protocol-spec.md is a check in `SessionLog` or in `check_result`, or the design marks it planned. Checked values are types: a `Delegation` is canonical when parsed, and `ServerSeeds` is checked when built.

### 2. "Zero life is dead" holds by convention only

A max-life rise lifts a dead unit's life above 0, so an area kills the corpse again. `ctx.kill` sets life to 0 and the unit dies at the next tick's deaths, as its API says, but that tick's regen runs first and the unit lives. Damage queued after the pass sits in a scratch past the tick's end, which a snapshot drops.

Target: one living test (not `Dead` and life above 0) that the pass, regen, the refresh and the attacks read. A life at 0 stays at 0 until the death or the respawn. A scratch refuses a push outside the window that drains it.

### 3. A restore check covers less than the load and the systems take

09-determinism-core.md says a restore gives an error, never a panic (A snapshot is what the hash reads). These restores pass and then panic or diverge: a box body on a spatial map, `Points` past the level, a user without its use slot, an `ActionStart` charge outside 0 to 1, a unit with a clipped weapon and no `ClipShots`. Navigation's `StaticChanges` is the diff of a cache against its last fill, so a restored match re-asks routes the original did not.

Target: each fact the load refuses, and each relation a system `expect`s, is a check of the part that holds it. No state that a restore cannot rebuild outlives the run that made it: a fresh cache's first fill is no change.

### 4. Rounded intermediates feed a second rounding

`Vec3::normalized`, `SinCos::of_degrees`, `Direction::length`, `rotated_deg` and `Bounds::exit` each claim one rounding and chain two or three. Their tests compare against the same chain. The box tests take an unsigned distance where the circle tests take a signed one, so a walker of radius 0 walks through every building.

Target: every operation derives from math's one wide primitive and rounds once, each doc states its bound in bits, and each differential test compares against an exact reference. "Closer than r" means "enters the open shape grown by r" for every shape.

### 5. A module's additions are not one walk of one list

The mode is outside `Release::modules`: `declare` and the install walk two lists, and `MatchEnd::stop_stages` goes into the schedule by hand. `Units::open` inserts five non-sends by hand, because `Declarations::non_send` cannot say "only in some roles", and the resource audit cannot see non-sends. `Book::OWNER` restates the declaring module. The kinds the release runs are written three times. `hold_charges` installs on `Combat`, where it needs nothing of combat.

Target: one list of modules, the mode included, walked once for declare, install, audit and binding. `non_send` returns an `Option`. The owner of a book is the module that declares it. Each install gate names the capability whose resources the system reads.

### 6. The registry is not the one source of what a script load checks

`ScriptFacts` sees only method-style calls, so `damage(ctx, …)` passes the load, and the release binds every capability's API into every match. Times and player slots have no `ScriptArg` type, so each capability resolves them by hand with three return shapes. `DATA_ACCESSORS`, Rhai's `get$` spelling and the hook arity are kept by hand beside the registry. There are two call flows: `hook_call` and a hand-made begin, call and apply in mode and orders.

Target: `ApiBuilder` records everything the load reads. Every argument that needs a check is a `ScriptArg`. The match binds only its declared capabilities. One call entry returns a `CallError` with a "pool spent" case.

### 7. A pass visits everything, not what changed

The Direction says "a pass visits what changed". `ActionSlots` holds the slot table and the per-action progress, so every attack and channel step re-triggers the change-driven hold passes and replication. `DueRun` does not deduplicate, so the heap grows each tick. Many passes scan every unit each tick: dying, held modifiers, attacks (three times), nodes, supply, AI thinking, the shop, placement, `keep_in_bounds`. In the client, the panels, the HUD and the bars rebuild each frame.

Target: the progress is its own component. One due-run scheme with a dedup. Each pass takes `Changed`, `Added` or an index.

### 8. An entity does not spawn whole

A unit moves between archetypes up to eight times, as `KindSpawn` reads the core back from the entity. Also: a player's `Seated`, a timed spawn's `Lifespan`, `Rally`, a grant's `TimedSlots`, a QUIC link's `RemoteId` and `Linked`.

Target: `KindSpawn` takes the core values, and each spawn is one bundle. A part every producer carries is declared and written in place.

### 9. Names and paths cross the boundary as strings

Bot orders look up unit types by name each time an order is due. The importer and the client build package paths with `format!`, then parse them with `expect`. The client-file types return `String` paths, and some files have no path function. A dependency path is a raw string joined as an OS path. Damage types are `&str` plus indices.

Target: each name resolves once, at the load or the start. Each format type owns its `PackagePath`. Each name of the original becomes an engine name or a path through one checked function.

### 10. Sibling types grew one at a time

The three store writers report failures in three shapes, and two of them lose the last failure at drop. `DurableFile::create` and `DurableDir::create` mean opposite things. Areas and projectiles differ in each role. Math has six mul and div entry points with no pattern. Nine INI readers copy one block grammar. The two client panels copy one widget.

Target: one contract for each family, and the siblings implement it.

### 11. Two paths build one thing

The net server builds a match on start and on restore with parameter bags, and seats a player twice and attaches the journal twice. `SessionLog` has 27 fields and seven jobs, and a rewound log is a hidden mode. The importer threads one mutable `ImportNames` through every stage, so the stage order is a hidden contract. `ModeSpec` copies package facts, and the spec is reached three ways.

Target: one `MatchRun` that both paths produce. `SessionLog` composes an entry store, a slot controller, an input schedule and segments. The importer builds the closed set of unit types first, and the stages return their additions.

### What the decisions follow

Each decision this plan holds, and the practice it follows.

- **A joined chain's root from the `Join`'s place.** A sequence that restarts at a session boundary lets old messages be replayed; the usual fix is a value unique to each session, given at resumption, as TLS's resumption nonces are.
- **Damage after the pass kept as state.** A restore or a rollback needs the whole state, pending events included (GGPO's save and load contract).
- **One living test; regen does not lift 0.** A unit at 0 life that still takes heals or targets is a known class of bug (Spring's dying units stay targets); the fix is one place that decides death, which every other system reads.
- **A spent pool fails an action's hook.** Warcraft III ends a thread past its operation limit, StarCraft II stops a thread past its budget with a trigger error, and Roblox ends a script that does not yield. None defers the call.
- **`on_interval` effects planned.** Unreal's Gameplay Ability System runs periodic effects from data alone, and Dota 2 from script (`OnIntervalThink`). A data-only import, as Zero Hour's toxins, needs the data form; the slice that imports them builds it.
- **Floats only in the importer.** Photon Quantum converts floats only in the editor, never in the simulation, and marks its float conversions unsafe.
- **Tint the look.** Zero Hour, Age of Empires and Timberborn tint the ghost; StarCraft II also colors each cell of its footprint grid, and Zero Hour marks the body that blocks.
- **Refuse unseen ground.** Zero Hour refuses a build point the player does not see, so a hidden body never causes a refusal.
- **`overflow-checks` kept.** Its cost is often small but can stop vectorization; an optimization session measures it.

## Plan

Each batch is one commit and ends with `./drive.py check` passing. Whoever addresses an item deletes it.

- **Tests.** Each bug item lands with a test that fails before its fix, built on the item's failure scenario. Each changed behavior is tested; a pure refactor is not.
- **Designs.** Where an item's code and its design disagree, the batch changes the one the item names, in the same commit.
- **Goldens and formats.** A batch marked "moves goldens" blesses them and says why each moved. A batch marked "format" raises the version tag of each format it changes. Any other batch moves no golden.
- **Order.** By impact: what lets a wrong log verify, then what makes a match diverge or panic, then what breaks a rule of the Direction, then the cleanups. A batch that needs another names it.

### 9. Mode and production

`mode`: `mode_effect`, `mode_call`, `mode_api`, `mode_spec`, `timers`, `loadout_setup`, `roster`, `mode_schema`, `mode_input`, `mode_hooks`, `mode_map`; `production`: `gather_loop`, `placement`, `production_orders`, `trains`, `player_supply`, `production_column`, `construction`, `gatherer`, `production_api`; `combat`: `dying`.

- [ ] production/src/placement.rs:51-57, 263-306; docs/design/04-capabilities/production.md, A seen enemy blocks, a friendly or hidden one moves out — a static body blocks a placement whether the builder's team sees it or not, so a build into fog reveals a hidden enemy building. Zero Hour (TheSuperHackers' `BuildAssistant::isLocationLegalToBuild`) refuses a build point the player does not see, and reports a fogged blocker as shroud, "so that players cannot use this info to determine whether they exist". → A placement passes only where the builder's team sees every cell under its box; a hidden body is then never a refusal's cause. That decision changes; the client's preview refuses unseen ground, from the same test. The server's fog of the Vision stage becomes a cache both roles build: the client builds its own team's, from the sights of the units it holds, the map's brush and the reveals it receives, and the allied sights and reveals reach it as the server holds them.

### 18. Zero Hour import: structure

`import`: `zero_hour` (the readers, the conversions, `error`, `import_names`, `imported_type`, `rules_package`, the INI readers, `model_import`), `texture`; `mode`: `mode_data`; `capabilities`: `books/unit_type_data`. After batch 17. Moves the Zero Hour fingerprint, which a run on Linux and one on macOS pin.

- [ ] import/src/zero_hour/imported_type.rs:18-68, import_names.rs:39-81, rules_package.rs:38-166; mode/src/mode_data.rs:26, capabilities/src/books/unit_type_data.rs:20 — `units.toml`, `mode.toml` and the manifest are written by hand-mirrored structs and `format!` templates, as the engine types derive `Deserialize` only. → The engine types derive `Serialize`; the importer builds them, as it builds `MapData`. This item moves the Zero Hour fingerprint, as the engine's field order is not the mirrors': `FINGERPRINTS` takes the new value of a run on Linux and one on macOS (12-zero-hour.md, One reproducible package).

### 20. Small capabilities: one shape for siblings

`orders`, `areas`, `projectiles`, `progression`, `interaction`, `items`, `production`: `production_orders`; `players`: `tick_commands`; `navigation`: `route`; `actions`: `targets`; `capabilities`: `book_builder`. After batches 11 and 19.

- [ ] orders/src/unit_order.rs:171-246 — the orders effect writes navigation's `Route`, `Progress`, `PathWalker` and production's `Builder`, `Gatherer`. → Navigation's work through contract parts of `units`: a path order (leave or rejoin) and a group's ask on `Destination`, which navigation pursues before its first read, so no golden moves; production's build and gather orders stay, as (02-engine-core.md, Contracts through `units`) says.
- [ ] orders, items, progression, production — `OrderInputs`/`PlayerOrders` against `<X>Orders`; `OrdersSet::Orders` against `<X>Set::Commands`. → One naming.
- [ ] orders/src/error.rs:10, ai.rs:30 — `AiError::TimeTooLarge` is its own case. → `Time(#[from] TimeTooLarge)`.
- [ ] items/src/trade_orders.rs:53-61, progression/src/learn_orders.rs:30-39, orders/src/order_inputs.rs:317-324, production/src/production_orders.rs:73 — each resolves id → entity → query → `controls(owner)`. → A `TickCommand` method in `players` that yields the controlled entities.
- [ ] areas/src/lib.rs:136-144, areas_effect.rs:15, projectiles/src/lib.rs:140, 199, launches.rs, projectiles_api.rs:63-66 — the two deliveries differ: a raw scratch against a wrapped one, a `pub` effect against a `pub(crate)` one, a six-parameter `push`; "launch" means a listed effect and a spawn queue; homing is held in the shape and in the spec. → One shape: a wrapped scratch, a `pub(crate)` effect, `push(launch)`; the queue renamed; homing read from the delivery.
- [ ] projectiles/src/lib.rs:104-107, areas/src/lib.rs:108-110 — `take_shots`, `launch` and `land` run on the client, which never fills them. → `cx.server`.
- [ ] projectiles/src/lib.rs:47, flying.rs:61; actions/src/targets.rs:22 — "It builds on combat" is false; the one use of stats is a `Without<Life>` filter, forced as `Targets` requires `&Life`. → Fix the doc; disjoint by `With<DeliveryEntity>`.
- [ ] progression/src/progression_column.rs:26, 54-58, track_book.rs:34, 103 — a `book` field refreshed and never read, `TrackBook::same` for it, and a doc that describes it. → Remove.
- [ ] progression/src/learning.rs:46; client/src/hud/mod.rs:92, 581 — charges come from progression's `Learning`, so a mode without progression shows none. → An actions-side reader of charges.
- [ ] items/src/shopping.rs:31, 105 — `ShopUnits` is every living unit, and the client calls `nearest_for` twice a frame. → A shop-only index.
- [ ] orders/src/thinking.rs:71-90 — `think` walks every AI unit each tick for the few due. → A due run.
- [ ] items/src/inventory.rs:58, capabilities/src/books/book_builder.rs:1017 — items extends a spawn's slots, and the top crate builds interaction's use slot. → `UseSlot::extend_slots`.
- [ ] areas/src/lib.rs:118, projectiles/src/lib.rs:115, production/src/lib.rs:223-237 — engine tag numbers are assigned by hand per crate to match the module list. → `declare` assigns them in declaration order.

### 21. Client

`client`: `hud`, `view` (`mod`, `unit_models`, `client_data`, `terrain_meshes`, `camera_rig`), `command_panel`, `shop_panel`, `inventory_bar`, `orders`, `build_preview`, `selecting`, `main`, `args`, `local_keys`, `ui_style`. After batches 16 and 20.

- [ ] client/src/hud/mod.rs:442-456 — `face_camera` gives each stand the camera's rotation as its local rotation, but a stand is a child of a root marked `Turns` (view/mod.rs:369, 780-783). A unit's life bar turns with it and is edge-on at 90°. → The stand's world rotation is the camera's (remove the root's yaw), and the system runs only when the camera, a yaw or the stands change.
- [ ] client/src/orders.rs:153-166 — Q/W/E/R cast slots 0 to 3 whatever their kind, where item keys find their slots by kind. A mode that declares `weapon` first fires the weapon on Q. → The ability keys find the first four ability slots, as the item keys find theirs.
- [ ] client/src/view/unit_models.rs:186-196 — a texture's sampler is set on the shared image, keyed by path, so the first load's wrap mode wins. → The sampler goes with the material: one image asset per sampler.
- [ ] client/src/command_panel.rs:192-290, inventory_bar.rs:98-136, shop_panel.rs:121-188, view/mod.rs:599-626, 720 — each frame the panels walk the selection and reformat every line, and `lean` and `ring_selection` walk every unit. → Each is gated on `Changed`/`Added` of what it reads; per frame only progress and windups.
- [ ] client/src/view/client_data/mod.rs:29-165 — `ClientData` lives the whole match but holds startup inputs (the whole `TerrainFile`, 3.8 MB), copies the flips into `GroundHeights`, and keeps two parallel `Vec`s by index; `toml` takes a parser that every caller passes alike, and a `&str` path that it parses each time. → A per-package record; startup inputs taken by their systems; `GroundHeights` alone stays.
- [ ] client/src/main.rs:206-258, action_names.rs:24, item_names.rs:88, view/terrain_view.rs:28, local_keys.rs:109, command_panel.rs:63 — four plugins each hold `Arc<ModePackages>` in a private resource (two identical `NamedPackages`); `add_ends` rebuilds the resource names the match holds in `ModeNames`. → One packages resource; read `ModeNames`.
- [ ] client/src/view/unit_models.rs:90-110 — `place_objects` resolves the look of each object instance (a `Vec`, an `Arc` copy, a path `String`, a lookup). → Resolve once per look, as `UnitLooks` does.
- [ ] client/src/view/mod.rs:39, 707-761, hud/mod.rs:628-663 — two ground-ring systems with their own meshes and trackers; `view` depends on `selection` for one of them. → One ring overlay in the HUD; cut view → selection.
- [ ] client/src/command_panel.rs:65-308, shop_panel.rs:40-218 — the panels repeat their types and spawns, and read the player's amount two ways. → One panel widget in `ui_style`; one accessor (the own slot).
- [ ] client/src/view/camera_rig.rs:77-80, selecting.rs:53 — two click rules. → One.
- [ ] client/src/view/mod.rs:69, 289-295 — a map with no terrain draws the MOBA lane's 44 × 20 m plane, and the doc says `View` owns the camera. → Size from the map's bounds; fix the doc.
- [ ] client/src/inventory_bar.rs:53, 118 — `SLOTS = 6` and the labels restate `ITEM_KEYS`. → Derive them.
- [ ] client/src/hud/mod.rs:248 — `Fills` claims every mesh transform. → A `GaugeFill` marker.
- [ ] client/src/view/mod.rs:333, build_preview.rs:127, command_panel.rs:316 — `Option<Res<UnitLooks>>` for a resource that always exists. → `Res`.
- [ ] client/src/command_panel.rs (about line 508) — `site_percent`'s `expect` names an invariant it does not test. → Assert the relation, or clamp.
- [ ] client/src/args/mod.rs:21, 151 — `team` returns a `&'static str` error; `spectate: bool` selects behavior. → An error enum; parse the role at the boundary.
- [ ] Wide signatures: `TerrainMesh::quad` (terrain_meshes.rs:172), `press_pan` (camera_rig.rs:173), `ground_under` (239), `Shown::fill` (command_panel.rs:414), `cancel` (379), `spawn_row` (shop_panel.rs:191). → Param structs.
- [ ] client/src/build_preview.rs:41-45, 158-164 — the production slice's design drew the building's look red where its box overlaps; the code leaves the look plain and draws a separate tinted footprint box. → The pipeline gains a general per-drawing tint over the look's materials, which any game may use; the preview tints the look, and the footprint box goes.

### 22. Published surfaces

Every capability crate's `lib.rs`; `net`. Last of the code batches, as the batches above move items. `source/crates/capabilities/AGENTS.md`: "an item another crate uses, this crate's tests among them, is `pub`, and the rest `pub(crate)`".

- [ ] Narrow to `pub(crate)` and remove from `lib.rs`: abilities `AbilitiesApi`, `Casts`; areas `AreaHolds`, `AreaLaunch`, `AreaLaunches`, `AreasApi`, `AreasEffect`; interaction `InteractionApi`; items `CarrierSlots`, `ItemHolds`, `ItemsApi`; orders `Destinations`, `OrderInputs`, `PlayerOrders`, `OrdersApi`, `Thinking`, `OrderedUnit`; progression `LearnOrders`, `ProgressionApi`, `ProgressionColumn`, `ProgressionEffect`, `Points::gain`; projectiles `LaunchPayload`, `Launches`, `Launching`, `Payload`, `ProjectilesApi`; combat `CombatApi`, `CombatEffect`, `CombatEvent`, `CombatEvents`, `CombatHooks`, `ModifierHooks`; production `Construction`, `GatherLoop`, `Trains`, `ProductionOrders`, `ProductionApi`, `ProductionColumn`, `Step`, `Start`, `BuildSpec`, `Required`, `Progressed`, `SupplyUnit`, `UnitSupply`; mode `Calls`, `ModeApi`, `ModeCall`, `ModeHooks`, `MatchMode`, `ModeSpec`, `Choices`, `ModeRefusal`, `SaveAsked`, `DeathCall`, `UnansweredDeaths`, `Timer`, `GameMap`, `Marker`, `MarkerInfo`. Each name a batch above already removed or renamed drops from this list.
- [ ] Behind `internals`: combat `DamagePass::restore` (damage_pass.rs:397), `Attacks::strike` (attacks.rs:230); net `JournalWatch`, `InputChannel`. To `pub(crate)`: net `SessionJournal`, `SnapshotDir`, `SessionFiles` and `RestoredSession.files`; net `SessionDir::create/start/restore/remove/publish`.
- [ ] Spec field visibility: `AreaSpec` all `pub`, `ProjectileSpec` mixed, `UseSpec` and `Shop` private with accessors. → One rule.
- [ ] A release test: each module's effect is `<Module>Effect`, and each `pub use` item is named by another crate.

### 23. Design documents

The designs that no batch above changes. Each names where code and design disagree, and which one changes.

- [ ] docs/design/02-engine-core.md, Dependencies and the `common` row — `common` depends on `blake3` and `sha2`, and holds the hashers, `Version`, `MapName`, `AfterLeave`, `Secret` and the `NamedEnum` trait. → The design changes.
- [ ] docs/design/02-engine-core.md:263 — lists the package index under Postcard; it is a text manifest. → The design changes.
- [ ] docs/design/02-engine-core.md:154 — lists `items` in `campfire-mode`'s Uses; `Mode::USES` has none. → The design changes.
- [ ] docs/design/04-capabilities/production.md — Cost lists supply passes only in a train's tick or a view read; Network requires a derivation each tick. → One rule, after batch 9.
