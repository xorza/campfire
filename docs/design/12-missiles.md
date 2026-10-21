# Missiles

A proposal for one slice. When it ends, each lasting decision moves into the document of the system it governs, and this document is deleted ([A slice's proposal is temporary](../../AGENTS.md#documents)); its plan is in `PLAN.md`. Its facts are in [Zero Hour missiles](../research/zero-hour-missiles.md).

**At the slice's end** a player who trains Tank Hunters, Missile Defenders, Stinger soldiers, tunnel defenders, rocket buggies, Tomahawks or Scud launchers, or builds a Patriot battery, sees them fire missiles that leave on a straight run, turn and speed up toward their targets, and strike them, as the original's missiles do; a rocket buggy, a Tomahawk or a Scud launcher too near its target backs away before it fires, and a Tomahawk's or a Scud's blast wounds what stands around it less than what it strikes.

## Decisions

### Projectiles

- **A steered flight.** A projectile type's `steer` flies it as a missile: it leaves at its `launch_speed`, gains its `acceleration` each second up to its `speed`, flies straight for its `straight` meters, and then turns its heading toward its target by at most its `turn_rate` degrees a second, in space, as Zero Hour's `THRUST` locomotor, OpenRA's `Missile` and Spring's `MissileProjectile` each speed up and turn. Refused: the homing flight with an acceleration, which flies a straight line with no launch arc and no turn; a curve, fixed as it launches, which a moving target leaves.
- **A lock makes the hit sure.** Within its `lock` meters of its target, on the ground between their centers, it flies straight at the target at its speed and hits it as a homing flight does, as Zero Hour's `KILL` state brakes onto its goal and OpenRA's `AllowSnapping` jumps onto it. Refused: a hit by collision alone, as Spring's, under which a missile of a wide turn may circle a target it never meets.
- **A cruise height.** With `cruise`, it steers toward the point its `height` above the ground over its target until it comes within its `dive` meters on the ground, and then toward the target, as Zero Hour's `PreferredHeight` and `DistanceToTargetBeforeDiving` and OpenRA's `CruiseAltitude` hold a missile up.
- **Fuel ends every flight.** A steered type has `fuel_ms`; when it runs out, the projectile ends where it stands, its end delivered there as a shell's that lands, so no flight lasts forever. Refused: no limit, under which a missile that circles flies on; a fall to the ground, as Zero Hour's and Spring's, which an imported missile reaches only when it circles.
- **A lost target ends it with no hit,** as a homing flight is lost, and as Zero Hour's missile destroys itself with no detonation when its victim is gone (`airborneTargetGone`). Refused: a flight on to the target's last point, as OpenRA's and Spring's, which the original does not do.
- **It meets the units on its way.** Each tick, its sphere of half its width at its point meets the living units its `hits` selects, its target always, as a curve's does, so a missile strikes a structure in its path, as Zero Hour's detonates on what its `ProjectileCollidesWith` names.
- **It leaves toward its target.** Its first heading points from its source's center to its target's on the ground, raised by twice its climb over the flat distance, as Zero Hour raises a missile over its launcher's facing. Refused: the source's facing, which a shot does not carry and which an attacker turns to its target before it fires.
- **One state for its motion.** Its flight state holds its heading, a unit vector, its speed, the meters it flew and the ticks of fuel left, each step rounded once from the last, as the sim's state is hashed. Refused: a heading as angles, whose trigonometry each tick costs more than the one normalization of a turned vector.
- **A section like the curve.** `steer` is a section of the projectile, as `curve` is, and the load refuses it beside `homing`, `curve`, `gravity`, `range`, `stop_on_hit` or `once_per_cast`. Refused: one `flight` field for every kind, which would rewrite each package's `homing` for a rule the load already holds.

### Combat and actions

- **A second ring of splash.** A splash's `outer = { radius, damage }` deals its damage to each unit its `affects` selects whose volume lies within its radius and beyond the splash's own, as Zero Hour's `SecondaryDamage` within `SecondaryDamageRadius` (`dealDamageInternal`). Refused: a falloff with the distance, as Spring's `edgeEffectiveness`, which no imported game holds; a second splash of its own, which would strike the inner ring twice.
- **A minimum range.** An action's `min_range` refuses a target nearer than it, measured as its range is, at its start and as its unit scans; an ordered action whose target is too near walks its unit straight away from the target to the point halfway between its minimum and its range, as Zero Hour's `computeApproachTarget`, and a unit that does not walk drops it. Refused: a minimum range of attacks alone, in `combat`, where the range of every action is checked in `actions` ([Actions](04-capabilities/actions.md), A range clamp or a walk).

### The import

- **A missile converts to a steered projectile.** A weapon whose `ProjectileObject` has a `MissileAIUpdate` and a `THRUST` locomotor fires a projectile type of its locomotor's `Speed`, or its weapon's `WeaponSpeed` with `UseWeaponSpeed`, `Acceleration` and `TurnRate`, its missile's `InitialVelocity`, `DistanceToTravelBeforeTurning`, `DistanceToTargetForLock`, 75 when absent, `FuelLifetime`, and `PreferredHeight` with `DistanceToTargetBeforeDiving`; its `hits` come from `ProjectileCollidesWith`, and its type's name and look from its object, as a shell's do.
- **A missile of no target flies to its point.** One of `TryToFollowTarget = No`, as the Scud's and the Strategy Center's, aims at the point where its target stood as it fired, its lock halved, as Zero Hour halves it for a point.
- **A secondary radius and a minimum range convert.** `SecondaryDamage` within `SecondaryDamageRadius` becomes the splash's `outer`, and `MinimumAttackRange` the attack's `min_range`.
- **What stays dropped.** A stream, a weapon of a `ProjectileStreamName`, as the toxin and the flame; a sprayer, as the toxin truck's; a weapon of a `ProjectileDetonationOCL`, as the Scud's poison fields and the Super Weapon general's Patriot pulse; a missile of an `IgnitionDelay`, of `DetonateOnNoFuel`, of a garrison kill, or of no fuel, as the Scud Storm's; each by its own cause, so the check counts what remains.

### The client

- **A missile draws as a shell does.** Its object's look at its point, turned along its last step; its exhaust and its contrail are lost, as the client draws no particles.
