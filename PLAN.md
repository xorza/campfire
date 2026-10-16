# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Zero Hour projectiles

The slice of [Zero Hour projectiles](docs/design/20-zero-hour-projectiles.md): the player trains a tank, and its shells destroy the bot's command center. The systems it changes: `geometry`, `units`, `client-files`, `mode`, `package`, `capabilities`, `projectiles`, `combat`, `import`, `client`, `zero-hour-check`. Each batch ends with `./drive.py check` passing; a golden that moves names its batch.

### 4. A curve's flight and its contact

`projectiles`: `projectile_data.rs`, `projectile.rs`, `flights.rs`, `flight_state.rs`, a new `curve/`. Builds: a pass visits what changed.

- [ ] crates/capabilities/projectiles/src/projectile.rs — a projectile flies homing or along a line (A projectile may fly a curve; A curve meets units by its point). → `Flight::Curve` of its four control points' inputs, its point count and its step; its point at step `k` from the closed form in one step; its length by the split rule; its end that follows a living target by `follow`; its end in the tick after its last point and as `life_ms` passes; after the tick's movement, its sphere at its point meets the volumes its `hits` select and its target's, never its source's, the lowest stable id first. The data section `curve`, refused with `homing` or `gravity`. The tests of [Checks](docs/design/20-zero-hour-projectiles.md#checks) for projectiles.

### 5. Splash, scatter and pitch

`combat`: `attacks.rs`, the attack's data, the damage queue; `projectiles`: the end of an attack's projectile. After batch 4. Builds: what a module adds is declared once.

- [ ] crates/capabilities/combat/src/attacks.rs:341-353 — an attack's projectile damages the one unit it hits, at its target always, and a shot reaches any pitch (An attack may splash, scatter and bound its pitch). → `splash`, `scatter` and `pitch_deg` in the attack's data, the first two refused without a projectile `delivery`; a projectile's end of an attack with a splash queues a damage for each living unit of its relations within its radius, in stable id order, and its hit none of its own; a scatter's two draws on their own stream give the ground point its projectile flies to, with no target to follow; a target out of the pitch bound is out of reach. The tests of [Checks](docs/design/20-zero-hour-projectiles.md#checks) for combat.

### 6. Turrets

`combat`: a new `turret/`, the attack's start; `client`: `view/unit_models.rs`. After batch 5. Builds: a kind is a marker; a part a system derives each tick is one it writes in place.

- [ ] crates/capabilities/combat/src/attacks.rs — an attack starts as soon as its target is in reach, and no part of a unit turns but its body (A unit type may have a turret). → A unit type's `turret`, a `TurretAngle` part on the types that have one, written in place each tick toward the target of the attack under way at its rate, back to 0 at half of it with none; an attack of the turret's slot kind starts its windup only within its tolerance; its owner's client predicts it. The client turns the look's `turret` node by it.

### 7. Slow weapons

`combat`: the attack's data, a held hit. After batch 6. Builds: what can be forgotten is a type.

- [ ] crates/capabilities/combat/src/attacks.rs:354-365 — an attack of no projectile lands its damage in the tick it fires, where Zero Hour delays it by `distance / WeaponSpeed` (A slow weapon delays its damage). → An attack's `speed`; its hit held as state until ⌈`d` ÷ `s`⌉ ticks after its shot, at once below one, on its target wherever it stands, none on a target dead by then. Takes the issue log's **Combat** item of Ready.

### 8. The import, the client's shells and the check

`import`: `zero_hour/combat_import.rs`, `zero_hour/imported_type.rs`, the models' conversion; `client`: `view/mod.rs`; `zero-hour-check`: its match and `pins.rs`. After batch 7. Builds: one rendering pipeline for every game; only the importer knows its game.

- [ ] crates/apps/import/src/zero_hour/combat_import.rs:218-223 — a weapon of a `ProjectileObject`, a radius or a scatter is dropped, and no object's turret, speed or pitch is read (The import converts shells, splash, scatter, pitch, turrets and slow weapons). → The conversions of the import's decision, the causes `Missile` and `Radius` for the weapons still dropped; the shells' models into their types' looks; the turret's bone into its unit's look. A fixture's test for each.
- [ ] crates/apps/client/src/view/mod.rs:441-471 — a projectile is drawn as one ball at a fixed height (The client draws a projectile with its look). → A projectile of a look draws its model at its point, its height included, turned along its last step; one of no look keeps the ball at its height.
- [ ] checks/zero-hour-check/ — no Zero Hour unit fires a shell, and the game package's fingerprint, which batch 2's `[ground]` moved to `69d19697…` on Linux, is pinned at its old value. → The check trains a Crusader, and its shells destroy the bot's command center; it pins its first shot's tick, its first detonation's and the import's counts. Its fingerprints are pinned from a run on Linux and a run on macOS ([Zero Hour](docs/design/12-zero-hour.md#decisions)).
