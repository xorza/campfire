# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Missiles

A player who trains Tank Hunters, Missile Defenders, Stinger soldiers, tunnel defenders, rocket buggies, Tomahawks or Scud launchers, or builds a Patriot battery, sees them fire missiles that leave on a straight run, turn and speed up toward their targets, and strike them; a unit too near its target backs away before it fires, and a blast's outer ring wounds less than its center ([proposal](docs/design/12-missiles.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The import of missiles (M)

`import`, the Zero Hour check, 09. First: it writes the steered flight, the outer splash ring and the minimum range, which the engine now holds.

- **The conversion.** `ImportedWeapon::of` (`apps/import/src/zero_hour/combat_import/mod.rs:315`) takes a `ProjectileObject` of a `MissileAIUpdate` and a `THRUST` locomotor, and `ImportedType::projectile` (`apps/import/src/zero_hour/imported_type/mod.rs:254`) writes its steered projectile from the proposal's fields, at its point for `TryToFollowTarget = No`; `SecondaryDamage` becomes the splash's `outer` and `MinimumAttackRange` the attack's `min_range`. Builds: a capability serves every game.
- **The drops.** `WeaponDrop` loses `Missile`, `Radius` for a secondary radius and `MinRange`, and gains `Stream`, `Sprayer`, `DetonationObjects`, `Ignition`, `NoFuel` and the others the proposal names; 09's losses name each.
- **Check:** tests of the conversion from INI fixtures: a guided missile's fields, a missile at a point, a secondary radius, a minimum range, and each new drop; `cargo drive run zero-hour`, on request, moves its fingerprints and its weapons' counts, and pins a play where a Tank Hunter's missile kills an enemy vehicle in its ticks.

### 2. Missiles in the client (S)

`client`, 04-capabilities/actions.md, combat.md, 09. Last: it ends the slice.

- **The look.** A steered projectile draws its object's look turned along its last step, as a shell does, or gains what it lacks; 09's losses name its exhaust and contrail.
- **The fold.** The decisions go to actions.md, combat.md and 09, the facts stay in research/zero-hour-missiles.md, and the proposal is deleted.
- **Check:** the user plays Tournament Desert in the client and sees a Tank Hunter's and a Missile Defender's missiles fly and strike, a rocket buggy back away from a near target, and a Patriot battery fire.

## Foundations
