# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Missiles

A player who trains Tank Hunters, Missile Defenders, Stinger soldiers, tunnel defenders, rocket buggies, Tomahawks or Scud launchers, or builds a Patriot battery, sees them fire missiles that leave on a straight run, turn and speed up toward their targets, and strike them; a unit too near its target backs away before it fires, and a blast's outer ring wounds less than its center ([proposal](docs/design/12-missiles.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. A second ring of splash and a minimum range (M)

`combat`, `actions`, `orders`, 04-capabilities/combat.md and actions.md. First: the two general options each later batch's import needs, in the systems that check damage and range today.

- **The outer ring.** `SplashData` (`combat/src/attack_data.rs:225`) gains `outer`, a radius at least its own and an amount; a projectile's end deals the amount to each unit its `affects` selects whose volume lies within the outer radius and beyond the inner one, once, after the inner ring. Builds: what can be forgotten is a type.
- **The minimum range.** An action's `min_range`, in its pipeline's fields beside `range` and refused by the load past it: the start step's range check (`core/actions/src/checked_action.rs:110`) refuses a target nearer, measured as the range is; an ordered action whose target is too near walks its unit straight away to the point halfway between `min_range` and `range` (`orders/src/destinations.rs`), and a unit that does not walk drops it; the idle scan (`orders/src/acquiring.rs`) passes over a unit inside it. Builds: one home for each operation.
- **Check:** tests of a splash's damage at hand-computed distances: inside the inner ring, between the rings, past the outer one, and a unit at both rings' edges; of a minimum range: a target refused inside it and taken at it, the walk to its midpoint by a hand-computed point, a unit that does not walk dropping the order, and a scan that passes the near unit for a farther one; the decisions marked planned in combat.md and actions.md become the proposal's.

### 2. The steered flight (L)

`projectiles`, `actions`, 04-capabilities/actions.md. After batch 1, independent of it; before batch 3, whose import writes it.

- **The data.** `ProjectileData` (`projectiles/src/projectile_data.rs`) gains `steer`: `launch_speed`, `acceleration`, `turn_rate` in degrees a second, `straight`, `lock`, `fuel_ms`, and `cruise = { height, dive }`; the builder refuses it beside `homing`, `curve`, `gravity`, `range`, `stop_on_hit` or `once_per_cast`, a launch speed past `speed`, and a fuel of 0. `ProjectileSpec` holds each a tick, the turn as the sine and cosine of its angle a tick, and `ProjectileFlight` (`core/actions/src/delivery.rs:36`) gains `Steered`, which aims at a unit or a point. Builds: a name is resolved once.
- **The flight.** `Flight::Steered` and `FlightState::Steered`, its heading, speed, meters flown and ticks of fuel; `take_shots` (`projectiles/src/launching.rs:23`) launches it from its source's center with the proposal's first heading; `TickFlights::fly` (`projectiles/src/flights.rs`) steps it: its speed by its acceleration to its top, its heading turned toward its goal by at most its turn, none within its straight run, its goal raised to its cruise height until its dive, its sphere meeting what `hits` selects along its step as a curve's does, straight at its target within its lock, and its end where it stands as its fuel runs out or its target is lost. Builds: a pass visits what changed.
- **Check:** tests of a flight's hand-computed steps: its straight run, its speed at each tick to its top, a turn held to its bound and one within it, its climb to its cruise height and its dive, its lock's last step onto its target, a structure met on its path, its end with no hit as its target dies, and its end as its fuel runs out; the load's refusals; the bench case `integration/projectiles/steered`, 1,000 missiles in flight for a tick, once in the gate.

### 3. The import of missiles (M)

`import`, the Zero Hour check, 09. After batches 1 and 2, whose data it writes.

- **The conversion.** `ImportedWeapon::of` (`apps/import/src/zero_hour/combat_import/mod.rs:315`) takes a `ProjectileObject` of a `MissileAIUpdate` and a `THRUST` locomotor, and `ImportedType::projectile` (`apps/import/src/zero_hour/imported_type/mod.rs:254`) writes its steered projectile from the proposal's fields, at its point for `TryToFollowTarget = No`; `SecondaryDamage` becomes the splash's `outer` and `MinimumAttackRange` the attack's `min_range`. Builds: a capability serves every game.
- **The drops.** `WeaponDrop` loses `Missile`, `Radius` for a secondary radius and `MinRange`, and gains `Stream`, `Sprayer`, `DetonationObjects`, `Ignition`, `NoFuel` and the others the proposal names; 09's losses name each.
- **Check:** tests of the conversion from INI fixtures: a guided missile's fields, a missile at a point, a secondary radius, a minimum range, and each new drop; `cargo drive run zero-hour`, on request, moves its fingerprints and its weapons' counts, and pins a play where a Tank Hunter's missile kills an enemy vehicle in its ticks.

### 4. Missiles in the client (S)

`client`, 04-capabilities/actions.md, combat.md, 09. Last: it ends the slice.

- **The look.** A steered projectile draws its object's look turned along its last step, as a shell does, or gains what it lacks; 09's losses name its exhaust and contrail.
- **The fold.** The decisions go to actions.md, combat.md and 09, the facts stay in research/zero-hour-missiles.md, and the proposal is deleted.
- **Check:** the user plays Tournament Desert in the client and sees a Tank Hunter's and a Missile Defender's missiles fly and strike, a rocket buggy back away from a near target, and a Patriot battery fire.

## Foundations
