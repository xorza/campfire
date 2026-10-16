# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Zero Hour projectiles

The slice of [Zero Hour projectiles](docs/design/20-zero-hour-projectiles.md): the player trains a tank, and its shells destroy the bot's command center. The systems it changes: `actions`, `stats`, `vision`, `geometry`, `units`, `client-files`, `package`, `projectiles`, `combat`, `import`, `client`, `zero-hour-check`. Each batch ends with `./drive.py check` passing; a golden that moves names its batch.

### 1. Living targets and two small fixes

`actions`: `src/targets.rs`; `vision`: `src/lib.rs`; `geometry`: `src/body_grid/mod.rs`. Independent of the others. Builds: what can be forgotten is a type.

- [ ] crates/capabilities/core/actions/src/targets.rs — a unit at zero life that has not died yet is a living target, where `stats`' one living test says it lives no more (D10). → Target choice reads that test, for attacks and casts; a test kills a unit by `ctx.kill` and finds it no target in the same tick. Takes the issue log's **Combat** item of Decide.
- [ ] crates/capabilities/vision/src/lib.rs — `Vision::clear_grid`'s doc says every unit then sees all, where a match with no grid runs no Vision stage and a unit with no `SeenBy` is seen by its own group alone. → The doc states what the code does. Takes the issue log's **Vision** item.
- [ ] crates/capabilities/core/geometry/src/body_grid/mod.rs — `BodyGrid::visit` and `visit_near` have no early stop, so `SeenNear::any` visits every body after its first hit. → Each takes a visitor that returns `ControlFlow`, and stops at `Break`; a test counts the bodies a stopped visit reads. Takes the issue log's **Geometry** item; D5's query uses it.

### 2. The ground in the sim

`geometry`: a new `ground/`; `client-files`: `height_grid.rs` goes; `units`: `map_book.rs`; `package`: the map's load; `client`: `view/ground_heights.rs`, `view/client_data/`; `import`: `zero_hour/map_import.rs`. After batch 1. Builds: a name is resolved once; each crate depends only on lower layers.

- [ ] crates/session/client-files/src/height_grid.rs; crates/apps/client/src/view/ground_heights.rs — the height grid is the client's alone, and its cells' splits live in the terrain file, so the sim measures no height (D1). → `geometry::Ground` holds the grid, each cell's split and the coarse grid of highest samples by `high_cell`; `Ground::height(x, z)` gives a triangle's height in one step, rounded once, and the nearest point's past the edge; `Ground::highest_along(a, b)` walks the coarse cells by Bresenham. `MapBook` holds the map's `Ground`, which the map's load reads from `map/<name>/heights.bin` and `[ground] high_cell`; the client's ground heights read it in place of their own and the terrain file's splits. The import writes the splits into `heights.bin` and `high_cell = "40"`. Design 16's D1 and 17's D8 name the new home.

### 3. A body's height

`units`: the unit type's body; `geometry`: the volume; `import`: `zero_hour/imported_type.rs`. After batch 2. Builds: an entity spawns whole.

- [ ] crates/capabilities/core/units/ — a body is a circle or a box of the ground plane, with no height, so a shot has no center to aim at and a splash no volume to reach (D2). → A unit type's `height` makes its body a volume; `Body::center` is its ground point, by `Ground`, raised by its height above the ground and half its own; a test of a volume's distance to a point in 3D, exact on squares (design 09, D9). The import writes each object's `GeometryHeight`.

### 4. A curve's flight and its contact

`projectiles`: `projectile_data.rs`, `projectile.rs`, `flights.rs`, `flight_state.rs`, a new `curve/`. After batch 3. Builds: a pass visits what changed.

- [ ] crates/capabilities/projectiles/src/projectile.rs — a projectile flies homing or along a line (D3, D4). → `Flight::Curve` of its four control points' inputs, its point count and its step; its point at step `k` from the closed form in one step; its length by the split rule; its end that follows a living target by `follow`; its end in the tick after its last point and as `life_ms` passes; after the tick's movement, its sphere at its point meets the volumes its `hits` select and its target's, never its source's, the lowest stable id first. The data section `curve`, refused with `homing` or `gravity`. The tests of design 20's Checks for projectiles.

### 5. Splash, scatter and pitch

`combat`: `attacks.rs`, the attack's data, the damage queue; `projectiles`: the end of an attack's projectile. After batch 4. Builds: what a module adds is declared once.

- [ ] crates/capabilities/combat/src/attacks.rs:341-353 — an attack's projectile damages the one unit it hits, at its target always, and a shot reaches any pitch (D5, D6, D7). → `splash`, `scatter` and `pitch_deg` in the attack's data, the first two refused without a projectile `delivery`; a projectile's end of an attack with a splash queues a damage for each living unit of its relations within its radius, in stable id order, and its hit none of its own; a scatter's two draws on their own stream give the ground point its projectile flies to, with no target to follow; a target out of the pitch bound is out of reach. The tests of design 20's Checks for combat.

### 6. Turrets

`combat`: a new `turret/`, the attack's start; `client`: `view/unit_models.rs`. After batch 5. Builds: a kind is a marker; a part a system derives each tick is one it writes in place.

- [ ] crates/capabilities/combat/src/attacks.rs — an attack starts as soon as its target is in reach, and no part of a unit turns but its body (D8). → A unit type's `turret`, a `TurretAngle` part on the types that have one, written in place each tick toward the target of the attack under way at its rate, back to 0 at half of it with none; an attack of the turret's slot kind starts its windup only within its tolerance; its owner's client predicts it. The client turns the look's `turret` node by it.

### 7. Slow weapons

`combat`: the attack's data, a held hit. After batch 6. Builds: what can be forgotten is a type.

- [ ] crates/capabilities/combat/src/attacks.rs:354-365 — an attack of no projectile lands its damage in the tick it fires, where Zero Hour delays it by `distance / WeaponSpeed` (D9). → An attack's `speed`; its hit held as state until ⌈`d` ÷ `s`⌉ ticks after its shot, at once below one, on its target wherever it stands, none on a target dead by then. Takes the issue log's **Combat** item of Ready.

### 8. The import, the client's shells and the check

`import`: `zero_hour/combat_import.rs`, `zero_hour/imported_type.rs`, the models' conversion; `client`: `view/mod.rs`; `zero-hour-check`: its match and `pins.rs`. After batch 7. Builds: one rendering pipeline for every game; only the importer knows its game.

- [ ] crates/apps/import/src/zero_hour/combat_import.rs:218-223 — a weapon of a `ProjectileObject`, a radius or a scatter is dropped, and no object's turret, speed or pitch is read (D11). → The conversions of D11, the causes `Missile` and `Radius` for the weapons still dropped; the shells' models into their types' looks; the turret's bone into its unit's look. A fixture's test for each.
- [ ] crates/apps/client/src/view/mod.rs:441-471 — a projectile is drawn as one ball at a fixed height (D12). → A projectile of a look draws its model at its point, its height included, turned along its last step; one of no look keeps the ball at its height.
- [ ] checks/zero-hour-check/ — no Zero Hour unit fires a shell. → The check trains a Crusader, and its shells destroy the bot's command center; it pins its first shot's tick, its first detonation's and the import's counts. Its fingerprints are pinned from a run on Linux and a run on macOS (design 12, D4).
