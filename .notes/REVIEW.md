# Workspace review

When you address an item, delete it. When a group is empty, delete its heading.

Paths are relative to `source/crates/`. Each item gives the place, the problem and a better shape. The groups are sorted by severity and benefit, and the items in each group by severity.

## 11. Data shapes allow states that should not exist

These types hold their rules in `expect`s, sentinels or loose fields, not in their shape.

- [ ] **`ctx.projectile` and `ctx.area` return `()`** — `capabilities/src/projectiles/projectiles_api.rs:60-63`, `areas/areas_api.rs:37-40`. Design 08 says a unit made in a call is usable in that call, and `chain_fire.rhai:12-13` uses the return value. Better: the frame takes the id at call time, as `spawn_unit` does.

## 12. Systems scan everything where a query or an index fits

These costs grow with all units or all entities each tick, while the work concerns a few.

- [ ] **Every script batch rebuilds every unit's row** — `capabilities/src/units/script_view.rs:147-195`, `scripts/script_batch.rs:29`. A tick can run about eight batches, each a full rebuild of every row from every row source. Better: keep the rows, and refresh only those whose source components changed since the last build.

## 13. Allocations on frequent paths


## 14. Coding guide breaks and small defects

- [ ] **One major struct per file** — `capabilities/src/actions/action_book.rs` (about ten types; `RankValues::all` builds a `LoadedRanks`); `combat/mod.rs` (about 810 lines with the damage-pass API); `units/script_view.rs` (eight types, three of them owned by stats and abilities); `units/unit.rs:200-291` (the `Pos` and `Vector` APIs); `stats/modifier_book.rs:32` (`ModifierId`); `stats/modifiers.rs` (`Instance`, `Application`); `stats/mod.rs` (700 lines, nine systems); `mode/mode_book.rs:326-340` (`SpawnAt`, `GroupUnit`); `mode/resource_id.rs:9-12` (`ResourceAmount`); `package/src/files/manifest.rs`; `package/src/mode_packages.rs` (`Dependent`).
