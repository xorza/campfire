# Workspace review

When you address an item, delete it. When a group is empty, delete its heading.

Paths are relative to `source/crates/`. Each item gives the place, the problem and a better shape. The groups are sorted by severity and benefit, and the items in each group by severity.

## 5. Navigation and vision have no limit on work per tick

The worst case per tick is the metric. These systems let one tick pay for a scan, a rebuild or a search that the other ticks do not.

- [ ] **Vision clears bitmaps by map size times groups each tick** — `capabilities/src/vision/mod.rs:101-115`. At 2²² cells and 256 teams, that is about 256 MiB of memset per tick, even with no units. vision.md says at most 64 groups, which the code does not enforce. Better: clear only the words the last tick touched, make a detection bitmap only for a group with a detector, and limit the groups.
- [ ] **Steering has no budget** — `capabilities/src/navigation/mod.rs:390-456`. The design says all avoidance runs with a fixed work limit a tick. Each blocked walker plans a window A* every tick, and one that cannot splice plans the same search again and again. Better: one planner budget for long and short routes, or a cap on short routes in stable-id order.
- [ ] **The route budget counts only expanded cells** — `capabilities/src/navigation/route_planner.rs:177-197,249-259`, `navigation/mod.rs:297,310`. `Regions::nearest` sorts every reachable region, and smoothing does a bucket scan per route cell. Neither counts. Better: charge them to the budget, or limit them.
- [ ] **A change of static bodies rebuilds all regions** — `capabilities/src/navigation/regions.rs:215-309`. Each rebuild copies every region and joins every chunk side, once per kind of walker. navigation.md says a change costs only the chunks it touches. Better: keep each region's cross-chunk edges and rebuild only dirty chunks, as 0 A.D. does.
- [ ] **Every route is tested again when bodies change** — `capabilities/src/navigation/mod.rs:224-241`. The test runs even when bodies are only removed, which can never block a route. Better: test routes only against the bodies added.
- [ ] **A path walker with an unreachable waypoint plans again every tick** — `capabilities/src/orders/mod.rs:377-388`, `navigation/mod.rs:492-497`. Its route ends short, `move_units` drops `Destination`, and `follow_paths` sets it again. The replicated `Destination` changes every tick. Better: keep a short route as "arrived short" until the static bodies change.
- [ ] **A short route is never planned again when a way opens** — `capabilities/src/navigation/mod.rs:234-241`. Only routes that bodies now block are planned again. Better: also ask again for unreached routes when bodies are removed.
- [ ] **Steering's blocker query has a fixed reach** — `capabilities/src/navigation/mod.rs:418`. It reaches twice the window, whatever the walker's width, so a walker wider than about 7.5 cells misses blockers. Better: compute the reach from the window and the walker's radius.
- [ ] **Steering visits every cell of a blocker** — `capabilities/src/navigation/route_planner.rs:280-290`. Better: clip each blocker's span to the window first.
- [ ] **Stuck walkers search each other in O(W²)** — `capabilities/src/navigation/mod.rs:427-441`. Better: a spatial lookup of walkers.
- [ ] **The body index does work it then throws away** — `capabilities/src/navigation/body_index.rs:196-269`. `meeting` computes each body's bucket range again for every entry, and `blocks` keeps searching after it finds a blocker. Better: store a first-cell flag in `Entry`, and let the visitor stop early.
- [ ] **`Progress` keeps the stuck count of the last walk** — `capabilities/src/navigation/progress.rs:143-156`, `navigation/mod.rs:394`. Better: reset it when a route is asked for or cleared.
- [ ] **Map-blocked cells are not in the exact tests** — `capabilities/src/navigation/pathing_grid.rs:37-41`, `navigation/route_planner.rs:119-130`. The tests know only bodies. Once a map blocks cells, smoothing and the straight-goal shortcut will cross them. Better: the exact tests also read the clearance.

## 7. Package books are copied into the script view, the frame and the state

Each item is a second copy of data that one book owns. The copies are kept in line by load order, by hand or not at all.

- [ ] **Instances store book data in state** — `capabilities/src/stats/modifiers.rs:51-53,75-80`. `Instance.tags`, and each change's stat and op, copy the book. They are hashed, snapshotted and trusted on restore, so a forged snapshot can grant any tags. Better: read them from the book by `instance.id`.
- [ ] **A unit type's tags are stored twice** — `capabilities/src/units/unit_types.rs:33-37,91-95,139-145`, `units/tag_book.rs:22`. After the copy into `TagBook`, production reads only that copy, and a later `give_tag` would diverge. `TagBook.type_tags` is a `ByType` though every type has tags. Better: one store, a `Vec<TagSet>` by type.

## 8. Capabilities import across layers

The design says a module imports only from the capabilities below it, and that the core names no capability. These imports break that.


## 9. Names from data stay strings

The rule is "no data in strings": a name from data becomes a checked type where it enters. These names do not.

- [ ] **Action data mixes strings and checked names** — `capabilities/src/actions/action_data.rs:47,52,65,68,71`, `actions/delivery_data.rs:14,19`. `hold`, `passive_modifier`, `unit_type`, the param and state keys and the delivery's unit type are `String`, while cost keys and `damage_kind` are `DeclaredName`. Better: all `DeclaredName`.
## 10. Parallel code paths apply one rule differently

One operation is written twice or three times, and the copies disagree.

- [ ] **A live change from a gone source drops to its base** — `capabilities/src/stats/refresh_scratch.rs:120-130`, `stats/mod.rs:495`. The doc says it keeps its last value. The fallback is the value at application, and a gone source resolves to the level-1 base. Better: decide the rule in stats.md, and keep the last value or document the base.
- [ ] **"A unit as a param source" is built three ways** — `capabilities/src/stats/param_source.rs:38-50`, `stats/param_sources.rs:28-38`, `stats/mod.rs:569-578`. Two treat level and stats as optional, one requires them. Better: one constructor.
- [ ] **A held and a timed copy of one modifier clash** — `capabilities/src/stats/modifiers.rs:116-146,247-255`, `stats/mod.rs:420-423`. An instance is keyed by (id, source) with one `held` flag, so one of the two is lost, and a self-applied copy of a passive's id makes the passive expire. Better: an instance that ends only when both its hold and its timer end.
- [ ] **"A unit type walks" is defined twice** — `package/src/mode_packages.rs:207-214`, `package/src/load_check.rs:119-128`. The avatar rules that `AvatarData` promises are applied in `runner/src/match_build.rs:200-205`. Better: `UnitTypeFile::walks()`, and an `AvatarData` method that gives the effective unit type.
- [ ] **A constant is copied into a message** — `capabilities/src/scripts/error.rs:429`. `ChainTooDeep` says "16 deep", which copies `MAX_DEPTH` in `stats/modifier_hooks.rs:23`.
- [ ] **`walk_to` repeats a framework method** — `capabilities/src/orders/mod.rs:437-443`. `walk_to` writes `set_if_neq` again.
- [ ] **`IndexedBody` and collision math are written several times** — `capabilities/src/navigation/mod.rs:118-123,188-193,368-373` build `IndexedBody` by hand three times; `navigation/collider.rs:436-454` computes `overlaps` twice in `part`. Better: `IndexedBody::new`, and one helper for the distance and reach.

## 11. Data shapes allow states that should not exist

These types hold their rules in `expect`s, sentinels or loose fields, not in their shape.

- [ ] **`Delivered` can name any hook** — `capabilities/src/deliveries/delivered.rs:14-15`, `deliveries/mod.rs:133-141`. Only `OnHit` and `OnEnd` are valid, `reached` is set only for a hit, and `run_hooks` reads `_` as `OnEnd`. `Projectile` and `Delivered` also split or copy what `Delivering` holds. Better: `enum Reach { Hit(StableId), End }` and `by: Delivering`.
- [ ] **A queued launch carries a placeholder group** — `capabilities/src/projectiles/mod.rs:230-235,319-334`, `combat/launches.rs:27-28`. `group: by.source` is overwritten in `launch`, and attack launches take cast numbers they never use. Better: a launch payload with no group, built into `Payload` once the first id is known.
- [ ] **`UnitKit` is built in two phases** — `capabilities/src/mode/unit_kit/mod.rs:49-133`. `new` gives a kit with four blank sections that four `with_*` calls fill, and the runner always calls all four. Better: one constructor.
- [ ] **Load errors have many shapes** — `package/src/error.rs:140,493-498`. `KindField` stands for about eight rules, five `Repeated*` variants say one thing, and `LoadError` is built by eight closures. Better: one case per rule, `Repeated { of, at, name }`, and `LoadError::new`.
- [ ] **Small shapes** — `Rc<OnceCell<Rc<ModeBook>>>` has an unused inner `Rc` (`capabilities/src/scripts/ctx.rs:25`); `ModeBook` keeps three `ByType` maps filled together (`mode/mode_book.rs:63-67,99-110`); `ModeSchema` keeps single-run `NameTable`s and a match-long `state_initial` (`mode/mode_schema.rs:11-27,42-53`); `UnitRow` keeps four start and end pairs where `ModifierRow` uses `Range<u32>` (`units/script_view.rs:130-141`); `ModePackages.units` adds a hop through `UnitsData`, and `PackageStore` hand-rolls a sorted map (`package/src/package_store.rs:13,26-35`); `PackageNames` keeps two maps on one key (`package/src/load_check.rs:43-45`). Better: one canonical shape each.
- [ ] **A kill is found by "the last death"** — `capabilities/src/combat/mod.rs:527-531`. `answer` relies on `deal` having pushed the kill last. Better: `deal` returns the killer and assisters in `Landed::Killed`.
- [ ] **`ctx.projectile` and `ctx.area` return `()`** — `capabilities/src/projectiles/projectiles_api.rs:60-63`, `areas/areas_api.rs:37-40`. Design 08 says a unit made in a call is usable in that call, and `chain_fire.rhai:12-13` uses the return value. Better: the frame takes the id at call time, as `spawn_unit` does.

## 12. Systems scan everything where a query or an index fits

These costs grow with all units or all entities each tick, while the work concerns a few.

- [ ] **Auras and player modifiers resolve names every tick** — `capabilities/src/stats/mod.rs:366-372,389-396`, `units/script_view.rs:658`. `apply_held` resolves each filter by a linear tag-name scan and each aura modifier by a string search, once per aura instance and once per owned unit for each player modifier. Areas already resolve these at load. Better: resolve them in `load_modifier`.
- [ ] **No delivery query is spatial** — `capabilities/src/projectiles/flights.rs:140`, `areas/mod.rs:183,251`, `combat/targets.rs:86-88`. Each line projectile tests every unit each tick, each area with `inside` modifiers scans every body each tick, and `Targets::units` fetches each row a second time. Better: one sorted cell index of bodies per tick, as the broadphase builds, and `units()` builds from the row it has.
- [ ] **Every script batch rebuilds every unit's row** — `capabilities/src/units/script_view.rs:231-295`, `scripts/script_batch.rs:29`. A tick can run about eight batches, each a full rebuild, and `deal_damage` reads the view every tick even with no damage. Queries are O(units) and parse their filter on every call. Better: rebuild only what a batch reads, and parse filters once.
- [ ] **Change detection on `Modifiers` is too coarse** — `capabilities/src/stats/modifiers.rs:192-238`, `combat/mod.rs:290-296,569-575`, `stats/mod.rs:197-213`. An interval, a shield absorb or a state write marks `Modifiers` changed, which re-derives the unit's stats and tags in full. Better: a "stats dirty" signal, or move timers, shields and state into their own component.
- [ ] **`cast_hits.keep` is O(hits × projectiles)** — `capabilities/src/projectiles/mod.rs:299`. `flying` is unsorted and repeats groups. Better: sort and dedup, or merge with `CastHits`.
- [ ] **Each projectile owns a `struck` list** — `capabilities/src/projectiles/projectile.rs:17,89-96`. It allocates on the first hit, is searched linearly in the hit loop, and duplicates `CastHits`. Better: one sorted flat store of (group, unit).
- [ ] **`RecentAttackers` scans its list on every damage** — `capabilities/src/combat/mod.rs:806-808`, `combat/recent_attackers.rs:21`. `record` looks up every entry in the index, and `respawn` drops its buffer. Better: prune on despawn or on read, and `clear()` on respawn.
- [ ] **The client marks every gauge and drawing changed every frame** — `client/src/hud/mod.rs:306-313,336`, `client/src/view.rs:317,368-371`. Better: `set_if_neq`, and skip finished glides.
- [ ] **A 0-stack live change refreshes its unit every pass** — `capabilities/src/stats/refresh_scratch.rs:76-95`. `add` does not skip `stacks == 0`. Better: skip it, as the tag derivation does.
- [ ] **Commands are parsed twice, and every tick** — `sim/src/command.rs:35-60`, `capabilities/src/orders/mod.rs:191`, `mode/mod.rs:444`. Better: validate once when the input is stored, and keep flat command ranges.
- [ ] **Small scans** — `Timers::fire` clones a repeating timer and removes from the front of a `Vec` (`capabilities/src/mode/timers.rs:59-73`, `mode/mod.rs:517`); the server builds a new `QueryState` every 2 ms frame (`server/src/main.rs:163`); a script `Unit` copies its whole row on each getter (`capabilities/src/units/unit.rs:33-37`, `units/script_view.rs:297-300`). Better: move the timer and use a heap, cache the state, and hold the row index.

## 13. Allocations on frequent paths

- [ ] **Each client order costs several allocations, and each input is hashed twice** — `net/src/sim_client/mod.rs:357-376`, `capabilities/src/orders/order.rs:41-44`, `sim/src/command.rs:22-31`. Better: `Command::payload` writes into a caller's buffer, and the chain is extended once.
- [ ] **`Route::clear` frees its waypoint buffer** — `capabilities/src/navigation/route.rs:481-483,497-498`. Every arrival allocates again, and `splice` moves memory twice. Better: clear the fields and keep the capacity, and one `splice`.
- [ ] **A stable sort allocates in each refresh pass** — `capabilities/src/stats/refresh_scratch.rs:114`. Order within a stat does not matter. Better: `sort_unstable_by_key`.
- [ ] **An applied handle allocates its state** — `capabilities/src/units/script_view.rs:461,463`. Better: reuse the frame's buffers.
- [ ] **`Modifiers` nests three `Vec`s per instance** — `capabilities/src/stats/modifiers.rs:20,45,51,55`. Every snapshot and rollback clone allocates per instance. Better: one flat buffer per carrier, after the book data leaves the instance (group 7).
- [ ] **Each fan allocates its flights** — `capabilities/src/projectiles/mod.rs:170-185`. Better: extend `Launches` directly.
- [ ] **Each script is parsed twice** — `package/src/package.rs:57`, `runner/src/match_build.rs:313-326`. Better: keep the AST and let the host take it.

## 14. Coding guide breaks and small defects

- [ ] **One major struct per file** — `capabilities/src/actions/action_book.rs` (about ten types; `RankValues::all` builds a `LoadedRanks`); `combat/mod.rs` (about 810 lines with the damage-pass API); `units/script_view.rs` (eight types, three of them owned by stats and abilities); `units/unit.rs:200-291` (the `Pos` and `Vector` APIs); `stats/modifier_book.rs:32` (`ModifierId`); `stats/modifiers.rs` (`Instance`, `Application`); `stats/mod.rs` (700 lines, nine systems); `mode/mode_book.rs:326-340` (`SpawnAt`, `GroupUnit`); `mode/resource_id.rs:9-12` (`ResourceAmount`); `package/src/files/manifest.rs`; `package/src/mode_packages.rs` (`Dependent`).
- [ ] **Narrowest visibility** — capabilities `lib.rs:86-88,95,134-148` re-exports `OnPath`, `PathEnd`, `Paths`, `NextThink`, `Block`, `TagEffect`, `TagData`, `Layer`, `PathId` and `RecentAttack`, which no other crate names; `TeamSet::ALL`, `of` and `union` and `PathId::index` are `pub`; `View::units_where` and `ModeBook::kit` are wider than their callers; `ModeBook.map` is `pub(crate)` beside its accessor; net `lib.rs:24-46` publishes `InputMessage`, `Join`, `Offer`, `MatchStart`, the channels, `Joined`, `JoinRefused`, `JoinState` and `UnitDied`.
- [ ] **Tuple return and long argument list** — `capabilities/src/scripts/frame.rs:337` returns `(&ParamTable, usize)`; `frame.rs:206-219` takes seven arguments behind an `expect`. Better: a result struct, and a `CallStart` struct.
- [ ] **`#[derive(Debug)]` missing** — the deserializer helpers in `capabilities/src/stats/stat_rule.rs:27`, `stats/stat_change.rs:20`, `values/bounds.rs:114`, `values/meter.rs:104`, `vision/vision_data.rs:18`.
- [ ] **`const fn` possible** — `StatTotals::change`, `Scalar::to_num`, `Ranked::ranks`, `get` and `values`, `Number::param`, `DeclaredName::as_str`, `UnitStats::values`.
- [ ] **The `bench` feature does not imply `internals`** — `capabilities/Cargo.toml:7-9`; `navigation/broadphase.rs:159` gates on `bench` directly.
- [ ] **The steering algorithm sits in a system function** — `capabilities/src/navigation/mod.rs:324-457`, `navigation/steering.rs`. Better: methods on `Steering` with private fields. `Waiting` (`navigation/route.rs:431-438`) is the planner's sort key and belongs with the planner.
- [ ] **Raw-bit arithmetic on `Num`** — `capabilities/src/navigation/progress.rs:144`, `navigation/mod.rs:388-389,434`, `navigation/body_index.rs:85`. Better: the `Num` operators, or a named method when truncation is the intent.
- [ ] **Wrong or stale comments** — `capabilities/src/abilities/mod.rs:60-62` says effects apply only in Resolve, but `ctx.apply` applies some at once; `combat/combat_events.rs:6-7` says abilities runs the event hooks, but stats does; `mode/mode_api.rs:527-528` has two summary lines; `mode/error.rs:28,77` leaves out markers; `units/script_view.rs:696-700` has a stray line from `slot`; doc lines in `navigation/mod.rs:65,210,315,514`, `navigation/broadphase.rs:9` and `navigation/route_planner.rs:13,20` pass 100 columns.
- [ ] **Small simplifications** — `Units::load_tags` only passes through (`capabilities/src/units/mod.rs:141-143`); `begin_tick` reaches into `ScriptFailures.0` (`units/mod.rs:161`); `Block::name` repeats serde's spelling (`units/block.rs:27-36`); `View::slot_count` belongs on `UnitRow`; the `if let Some(view)` checks in combat, stats, vision and navigation can never fail, and `load_modifier` needs the view, then treats it as optional (`stats/mod.rs:272,280,283`); `run_timers` checks `OnTimer` on every timer; `Mode::start` inserts `OnPath` after the spawn where `spawn_group` passes it in the bundle (`mode/mod.rs:404-411`).
- [ ] **Hidden rule** — the refresh clamps a pool maximum to `Num::EPSILON` (`capabilities/src/stats/mod.rs:602`), while the kit refuses a maximum that is not positive. stats.md states neither rule.
- [ ] **An invalid `RUST_LOG` falls back with no notice** — `log/src/logging.rs:34,44`. Better: fall back only when the variable is absent.
- [ ] **The empty `identity` crate pulls in `nostr`** — `identity/src/lib.rs`, `identity/Cargo.toml`. Better: drop its dependencies, or the crate, until it has code.
