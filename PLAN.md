# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Zero Hour projectiles

The slice of [Zero Hour projectiles](docs/design/20-zero-hour-projectiles.md): the player trains a tank, and its shells destroy the bot's command center. The systems it changes: `geometry`, `units`, `client-files`, `mode`, `package`, `capabilities`, `projectiles`, `combat`, `import`, `client`, `zero-hour-check`. Each batch ends with `./drive.py check` passing; a golden that moves names its batch.

### 8. The import, the client's shells and the check

`import`: `zero_hour/combat_import.rs`, `zero_hour/imported_type.rs`, the models' conversion; `client`: `view/mod.rs`; `zero-hour-check`: its match and `pins.rs`. Builds: one rendering pipeline for every game; only the importer knows its game.

- [ ] crates/apps/import/src/zero_hour/combat_import.rs:218-223 — a weapon of a `ProjectileObject`, a radius or a scatter is dropped, and no object's turret, speed or pitch is read (The import converts shells, splash, scatter, pitch, turrets and slow weapons). → The conversions of the import's decision, the causes `Missile` and `Radius` for the weapons still dropped; the shells' models into their types' looks; the turret's bone into its unit's look. A fixture's test for each.
- [ ] crates/apps/client/src/view/mod.rs:441-471 — a projectile is drawn as one ball at a fixed height (The client draws a projectile with its look). → A projectile of a look draws its model at its point, its height included, turned along its last step; one of no look keeps the ball at its height.
- [ ] checks/zero-hour-check/ — no Zero Hour unit fires a shell, and the game package's fingerprint, which batch 2's `[ground]` moved to `69d19697…` on Linux, is pinned at its old value. → The check trains a Crusader, and its shells destroy the bot's command center; it pins its first shot's tick, its first detonation's and the import's counts. Its fingerprints are pinned from a run on Linux and a run on macOS ([Zero Hour](docs/design/12-zero-hour.md#decisions)).
