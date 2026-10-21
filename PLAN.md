# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Missiles

A player who trains Tank Hunters, Missile Defenders, Stinger soldiers, tunnel defenders, rocket buggies, Tomahawks or Scud launchers, or builds a Patriot battery, sees them fire missiles that leave on a straight run, turn and speed up toward their targets, and strike them; a unit too near its target backs away before it fires, and a blast's outer ring wounds less than its center ([proposal](docs/design/12-missiles.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The steered flight (L)

`projectiles`, `actions`, 04-capabilities/actions.md. First: the flight batch 2's import writes.

- **The data.** `ProjectileData` (`projectiles/src/projectile_data.rs`) gains `steer`: `launch_speed`, `acceleration`, `turn_rate` in degrees a second, `straight`, `lock`, `fuel_ms`, and `cruise = { height, dive }`; the builder refuses it beside `homing`, `curve`, `gravity`, `range`, `stop_on_hit` or `once_per_cast`, a launch speed past `speed`, and a fuel of 0. `ProjectileSpec` holds each a tick, the turn as the sine and cosine of its angle a tick, and `ProjectileFlight` (`core/actions/src/delivery.rs:36`) gains `Steered`, which aims at a unit or a point. Builds: a name is resolved once.
- **The flight.** `Flight::Steered` and `FlightState::Steered`, its heading, speed, meters flown and ticks of fuel; `take_shots` (`projectiles/src/launching.rs:23`) launches it from its source's center with the proposal's first heading; `TickFlights::fly` (`projectiles/src/flights.rs`) steps it: its speed by its acceleration to its top, its heading turned toward its goal by at most its turn, none within its straight run, its goal raised to its cruise height until its dive, its sphere meeting what `hits` selects along its step as a curve's does, straight at its target within its lock, and its end where it stands as its fuel runs out or its target is lost. Builds: a pass visits what changed.
- **Check:** tests of a flight's hand-computed steps: its straight run, its speed at each tick to its top, a turn held to its bound and one within it, its climb to its cruise height and its dive, its lock's last step onto its target, a structure met on its path, its end with no hit as its target dies, and its end as its fuel runs out; the load's refusals; the bench case `integration/projectiles/steered`, 1,000 missiles in flight for a tick, once in the gate.

### 2. The import of missiles (M)

`import`, the Zero Hour check, 09. After batch 1, whose flight it writes, and the outer splash ring and minimum range, which it writes too.

- **The conversion.** `ImportedWeapon::of` (`apps/import/src/zero_hour/combat_import/mod.rs:315`) takes a `ProjectileObject` of a `MissileAIUpdate` and a `THRUST` locomotor, and `ImportedType::projectile` (`apps/import/src/zero_hour/imported_type/mod.rs:254`) writes its steered projectile from the proposal's fields, at its point for `TryToFollowTarget = No`; `SecondaryDamage` becomes the splash's `outer` and `MinimumAttackRange` the attack's `min_range`. Builds: a capability serves every game.
- **The drops.** `WeaponDrop` loses `Missile`, `Radius` for a secondary radius and `MinRange`, and gains `Stream`, `Sprayer`, `DetonationObjects`, `Ignition`, `NoFuel` and the others the proposal names; 09's losses name each.
- **Check:** tests of the conversion from INI fixtures: a guided missile's fields, a missile at a point, a secondary radius, a minimum range, and each new drop; `cargo drive run zero-hour`, on request, moves its fingerprints and its weapons' counts, and pins a play where a Tank Hunter's missile kills an enemy vehicle in its ticks.

### 3. Missiles in the client (S)

`client`, 04-capabilities/actions.md, combat.md, 09. Last: it ends the slice.

- **The look.** A steered projectile draws its object's look turned along its last step, as a shell does, or gains what it lacks; 09's losses name its exhaust and contrail.
- **The fold.** The decisions go to actions.md, combat.md and 09, the facts stay in research/zero-hour-missiles.md, and the proposal is deleted.
- **Check:** the user plays Tournament Desert in the client and sees a Tank Hunter's and a Missile Defender's missiles fly and strike, a rocket buggy back away from a near target, and a Patriot battery fire.

## Foundations
