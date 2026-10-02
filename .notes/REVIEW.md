# Workspace review

When you address an item, delete it. When a group is empty, delete its heading.

Paths are relative to `source/crates/`. Each item gives the place, the problem and a better shape. The groups are sorted by severity and benefit, and the items in each group by severity.

## 7. Package books are copied into the script view, the frame and the state

Each item is a second copy of data that one book owns. The copies are kept in line by load order, by hand or not at all.

- [ ] **A unit type's tags are stored twice** — `capabilities/src/units/unit_types.rs:33-37,91-95,139-145`, `units/tag_book.rs:22`. After the copy into `TagBook`, production reads only that copy, and a later `give_tag` would diverge. `TagBook.type_tags` is a `ByType` though every type has tags. Better: one store, a `Vec<TagSet>` by type.

## 9. Names from data stay strings

The rule is "no data in strings": a name from data becomes a checked type where it enters. These names do not.

- [ ] **Action data mixes strings and checked names** — `capabilities/src/actions/action_data.rs:47,52,65,68,71`, `actions/delivery_data.rs:14,19`. `hold`, `passive_modifier`, `unit_type`, the param and state keys and the delivery's unit type are `String`, while cost keys and `damage_kind` are `DeclaredName`. Better: all `DeclaredName`.
## 10. Parallel code paths apply one rule differently

One operation is written twice or three times, and the copies disagree.

- [ ] **"A unit as a param source" is built three ways** — `capabilities/src/stats/param_source.rs:38-50`, `stats/param_sources.rs:28-38`, `stats/mod.rs:569-578`. Two treat level and stats as optional, one requires them. Better: one constructor.
- [ ] **"A unit type walks" is defined twice** — `package/src/mode_packages.rs:207-214`, `package/src/load_check.rs:119-128`. The avatar rules that `AvatarData` promises are applied in `runner/src/match_build.rs:200-205`. Better: `UnitTypeFile::walks()`, and an `AvatarData` method that gives the effective unit type.

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

- [ ] **Every script batch rebuilds every unit's row** — `capabilities/src/units/script_view.rs:147-195`, `scripts/script_batch.rs:29`. A tick can run about eight batches, each a full rebuild of every row from every row source. Better: keep the rows, and refresh only those whose source components changed since the last build.
- [ ] **The client marks every gauge and drawing changed every frame** — `client/src/hud/mod.rs:306-313,336`, `client/src/view.rs:317,368-371`. Better: `set_if_neq`, and skip finished glides.
- [ ] **Commands are parsed twice, and every tick** — `sim/src/command.rs:35-60`, `capabilities/src/orders/mod.rs:191`, `mode/mod.rs:444`. Better: validate once when the input is stored, and keep flat command ranges.
- [ ] **Small scans** — `Timers::fire` clones a repeating timer and removes from the front of a `Vec` (`capabilities/src/mode/timers.rs:59-73`, `mode/mod.rs:517`); the server builds a new `QueryState` every 2 ms frame (`server/src/main.rs:163`); a script `Unit` copies its whole row on each getter (`capabilities/src/units/unit.rs:33-37`, `units/script_view.rs:297-300`). Better: move the timer and use a heap, cache the state, and hold the row index.

## 13. Allocations on frequent paths

- [ ] **Each client order costs several allocations, and each input is hashed twice** — `net/src/sim_client/mod.rs:357-376`, `capabilities/src/orders/order.rs:41-44`, `sim/src/command.rs:22-31`. Better: `Command::payload` writes into a caller's buffer, and the chain is extended once.
- [ ] **An applied handle allocates its state** — `capabilities/src/units/script_view.rs:461,463`. Better: reuse the frame's buffers.
- [ ] **Each script is parsed twice** — `package/src/package.rs:57`, `runner/src/match_build.rs:313-326`. Better: keep the AST and let the host take it.

## 14. Coding guide breaks and small defects

- [ ] **One major struct per file** — `capabilities/src/actions/action_book.rs` (about ten types; `RankValues::all` builds a `LoadedRanks`); `combat/mod.rs` (about 810 lines with the damage-pass API); `units/script_view.rs` (eight types, three of them owned by stats and abilities); `units/unit.rs:200-291` (the `Pos` and `Vector` APIs); `stats/modifier_book.rs:32` (`ModifierId`); `stats/modifiers.rs` (`Instance`, `Application`); `stats/mod.rs` (700 lines, nine systems); `mode/mode_book.rs:326-340` (`SpawnAt`, `GroupUnit`); `mode/resource_id.rs:9-12` (`ResourceAmount`); `package/src/files/manifest.rs`; `package/src/mode_packages.rs` (`Dependent`).
