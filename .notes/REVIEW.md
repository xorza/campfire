# Workspace review

When you address an item, delete it. When a group is empty, delete its heading.

Paths are relative to `source/crates/`. Each item gives the place, the problem and a better shape. The groups are sorted by severity and benefit, and the items in each group by severity.

## 11. Data shapes allow states that should not exist

These types hold their rules in `expect`s, sentinels or loose fields, not in their shape.

- [ ] **`ctx.projectile` and `ctx.area` return `()`** — `campfire-capabilities/src/projectiles/projectiles_api.rs:60-63`, `areas/areas_api.rs:37-40`. Design 08 says a unit made in a call is usable in that call, and `chain_fire.rhai:12-13` uses the return value. Better: the frame takes the id at call time, as `spawn_unit` does.

## 12. Systems scan everything where a query or an index fits

These costs grow with all units or all entities each tick, while the work concerns a few.

- [ ] **The state hash reads all state every tick** — `campfire-sim/src/state_registry`. In the 3v3 test a hash costs about 27 µs a tick, 31 % of the test's time, though most ticks change few rows. Better: keep a digest for each entity, update the digests of the entities whose state changed, and combine them, so an unchanged entity costs nothing.
- [ ] **A tick with nothing to do costs about 31 µs** — measured on the 3v3's pick ticks, where no unit acts. Better: find what dominates an empty tick, and make a system with no work cost nothing. The view's read is not it: it is about 4.5 µs a tick at 49 units.
