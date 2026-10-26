# Zero Hour missiles

How Zero Hour flies a missile, which weapons fire one, and how other engines model a guided missile, read on 21 October 2026: Zero Hour from GeneralsX's `GeneralsMD` source at `0e79905d1` and the INI files of the install `GENERALSZH_DATA` names, resolved as the import resolves them; the others from their sources as linked.

## The dropped weapons

- **52 by `Missile`.** The import drops a weapon whose `ProjectileObject` has no `DumbProjectileBehavior`; the Zero Hour check of 21 October 2026 counted 52 such weapons of the 121 it read, beside 33 imported. Every one of the 52 fires an object of a `MissileAIUpdate`.
- **Four kinds among them.** By their missile's data:
  - **Guided**, `TryToFollowTarget = Yes`: the rocket buggy's, the Stinger soldier's, the tunnel defender's, the Tank Hunter's, the Missile Defender's, the Patriot's, the Tomahawk's and the Hellfire drone's, and the MiG's, Raptor's and Comanche's, which fly.
  - **At a point**, `TryToFollowTarget = No`: the Scud launcher's, the Strategy Center's and the artillery platform's guns, and the Scud Storm's.
  - **Streams**, an object of `ProjectileStreamUpdate` or a stream locomotor, fuel of 350 to 600 ms: the toxin truck's and the tunnel network's toxin, the ambulance's and the Biohazard tech's cleaner, the Dragon tank's flame and fire wall, and the Rebel's toxin of the Chemical general.
  - **Sprayers**: the toxin truck's `ToxinTruckSprayer`, a missile of 1 ms of fuel that spreads contamination.
- **The guided missiles' data.** Each a `THRUST` locomotor:

| Weapon | Range | Damage / radius | Missile `Speed` / `Acceleration` / `TurnRate` | `InitialVelocity` | `DistanceToTravelBeforeTurning` | Lock | Fuel ms |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `ChinaInfantryTankHunterMissileLauncher` | 175 | 40 / 5 | 225 / 675 / 100 | 150 | 20 | 75 | 1,250 |
| `MissileDefenderMissileWeapon` | 175 | 40 / 5 | 225 / 675 / 100 | 150 | 3 | 75 | 3,000 |
| `BuggyRocketWeapon`, clip 6 | 300 | 20 / 0 | 225 / 675 / 100 | 150 | 3 | 75 | 1,800 |
| `TunnelDefenderRocketWeapon` | 175 | 40 / 5 | 225 / 675 / 100 | 150 | 20 | 75 | 1,250 |
| `StingerMissileWeapon` | 225 | 20 / 5 | 400 / 900 / 625 | 200 | 5 | 100 | 5,000 |
| `PatriotMissileWeapon`, clip 4 | 225 | 30 / 5 | 400 / 450 / 300 | 50 | 5 | 100 | 10,000 |
| `TomahawkMissileWeapon` | 350 | 200 / 10 | 200 / 675 / 540, `PreferredHeight` 120, dive at 100 | 50 | 80 | 10 | 4,000 |

- **The point missiles' data.** `SCUDLauncherGun*`: range 350, 200 or 300 damage in a radius of 30 or 50, `SCUDMissileLocomotor` of speed 200, acceleration 675, `PreferredHeight` 120, a straight 200 before it turns and a dive within 200 of its point. `StrategyCenterGun`: range 400, 200 damage in 25, `NuclearCannonShellLocomotor` of speed 300, a straight 40 and a dive within 50.

- **The other causes behind them.** The import tests a weapon's causes in turn, the missile first, so a missile weapon may hold others: `SecondaryDamageRadius`, the rocket buggy's 10, the Tomahawk's 25, the Scud's 60 or 100; `MinimumAttackRange`, the Tank Hunter's and the tunnel defender's 5, the rocket buggy's 50, the Tomahawk's and the Strategy Center's 100, the Scud's 200; a `ProjectileDetonationOCL`, the Scud's poison fields; `AntiGround = No`, the Stinger's and the Patriot's air weapons. Of the guided missiles of ground units, only the Missile Defender's, the Stinger soldier's ground weapon and the Patriot's ground weapon hold none.

## Zero Hour's secondary damage and minimum range

- **Two rings of damage.** A detonation deals `PrimaryDamage` to each victim within `PrimaryDamageRadius` and `SecondaryDamage` to each within `SecondaryDamageRadius` beyond it, the secondary radius at least the primary (`WeaponTemplate::dealDamageInternal`).
- **No shot within the minimum range.** A weapon does not fire at a victim nearer than `MinimumAttackRange` (`WeaponTemplate::estimateWeaponDamage`, `isTooClose`); a unit whose weapon's minimum range passes a pathfind cell, 10, moves straight away from a victim inside it to the point halfway between its minimum and its maximum range (`Weapon::computeApproachTarget`), and does not chase one it is already too close to.

## Zero Hour's missile

- **An object with a locomotor.** A missile is an object of `MissileAIUpdate` and `PhysicsBehavior`, which its weapon spawns at its launcher's barrel bone (`Weapon::positionProjectileForLaunch`) and pushes along the launcher's facing at `InitialVelocity`, its direction raised by twice the climb to its target over the flat distance; its `THRUST` locomotor then drives it at its `Speed`, its `Acceleration` and its `TurnRate`, as an AI move to its victim, or to its point raised 10 when it locks on a point (`MissileAIUpdate::projectileFireAtObjectOrPosition`).
- **States.** `LAUNCH` holds no thrust for `IgnitionDelay`; `IGNITION` arms it and starts its fuel; `ATTACK_NOTURN` thrusts with no turn until it flew `DistanceToTravelBeforeTurning`; `ATTACK` turns freely; within `DistanceToTargetBeforeDiving` a missile of a `PreferredHeight` stops holding its height (`doAttackState`).
- **The lock is a sure hit.** Within `DistanceToTargetForLock` of its victim, 75 by default, half that from a point, measured in 2D between centers, it enters `KILL`, where it brakes onto its goal exactly, "This ensures that the missile will hit", and detonates on reaching it (`doKillState`).
- **A scattered shot flies to its point.** A weapon of a `ScatterRadius` above 0 launches its missile at the scattered point, not at its victim, "overriding the intent" (`WeaponTemplate::fireWeaponTemplate`), so the missile tracks nothing and its lock is halved as for any point; the Zero Hour check of 21 October 2026 found a scattered missile among the rocket buggy's weapons.
- **Fuel.** At `FuelLifetime` it stops thrusting and turning and falls, or detonates with `DetonateOnNoFuel`; 0 is no limit.
- **A lost target.** A guided missile whose victim is gone runs out of fuel at once and destroys itself with no detonation (`airborneTargetGone`): no damage, where its comment says it cannot retarget.
- **Collisions on the way.** An armed missile detonates on the first object it collides with that its weapon's `ProjectileCollidesWith` names, as a shell does, or on the ground; the guided missiles above name `STRUCTURES` (`projectileHandleCollision`, `shouldProjectileCollideWith`).
- **Its look.** Its object's model, and an exhaust particle system its weapon names, which it drops as its fuel ends; it hides on detonation and is destroyed 3 frames later, so its contrail catches up (`KillSelfDelay`).

## Other engines

- **OpenRA's `Missile`.** A projectile that turns by `HorizontalRateOfTurn` and `VerticalRateOfTurn` a tick, speeds by `Acceleration` from its launch speed to `Speed`, and levels at `CruiseAltitude`; it locks on by `LockOnProbability` at launch, aims off by `Inaccuracy`, detonates within `CloseEnough` of its target, and with `AllowSnapping` jumps onto it within one tick's travel; a target that becomes invalid leaves it flying to its last position; past `RangeLimit`, its weapon's range by default, it falls, and explodes with `ExplodeWhenEmpty` ([`Missile.cs`](https://raw.githubusercontent.com/OpenRA/OpenRA/bleed/OpenRA.Mods.Common/Projectiles/Missile.cs)).
- **Spring's `MissileProjectile`.** It speeds by `weaponacceleration` to `projectilespeed` and turns by `turnrate` toward a lead point, `targetPos + targetVel · (targetDist / maxSpeed) · 0.7`, only when `tracks`; it hits by collision alone; past its time to live it falls under drag and gravity, or explodes with `selfExplode`; a dead target leaves it flying to its last point ([`MissileProjectile.cpp`](https://raw.githubusercontent.com/beyond-all-reason/spring/master/rts/Sim/Projectiles/WeaponProjectiles/MissileProjectile.cpp)).

## The engine's projectiles

- **Homing.** A homing projectile steps its type's `speed` a tick straight toward its target's position, hits when its circle of half its width reaches the target's body, and ends with no hit when the target dies, becomes no target or teleports; the load requires it faster than `max_move_speed` (`flights.rs`, [Actions](../design/04-capabilities/actions.md)).
- **A curve.** A cubic Bézier curve from its launch to its target's center, its inner points at fractions of the way and heights over the ground between, its end following its target by `follow` m/s, meeting the units its `hits` selects by its sphere; the import flies Zero Hour's `DumbProjectileBehavior` shells on it.

## The port's Tomahawk

- **A report of 26 October 2026.** In a match on Tournament Desert, a Tomahawk at (1,053, −1,446) fired four missiles at an ECM tank at (1,045, −1,815), 370 m away. Its replay, probed at 100 ms, shows each missile end 9 ticks after its launch, about 50 m out, with the tank at 300 of 300 life. Each started at the launcher's place, and the launcher's model kept its missile on its rail. Its missile left flat toward its target, where the original's leaves along its launch bone's `x`, raised by twice the target's rise over the ground distance (`MissileAIUpdate::projectileLaunchAtObjectOrPosition`), near upright from the Tomahawk's rail.
- **Why it ended (found the same day).** On a planar map a steered flight ends where its height over the ground falls below 0; the missile starts at its launcher's center, 5 m up, and its straight run heads at its target's center, so lower ground at the target, or a rise on the way, puts it under the ground within a few ticks. The same flight, in a test over level ground at a target 370 m away, climbs to its cruise of 120 m after its straight run of 80 m and strikes the target at tick 65.
- **Why it flew flat (found the same day).** The Tomahawk's `W3DTankDraw` names `Turret = TURRET`, `TurretPitch = TURRETEL` and `WeaponLaunchBone = PRIMARY WeaponA`, and its AI module's turret `TurretTurnRate = 60`, `TurretPitchRate = 60`, `AllowsPitch = Yes` and `FirePitch = 70`. The import's `avtomahawk.glb` holds `weapona01` under `turretel` under `turret`, at (7.165, 3.192, −0.503) of the model's frame on glTF's axes, its rest along `x`: the bone lies flat, and the turret pitches it to 70° before it fires, as an attack waits for the pitch (`TurretAIAimTurretState::update`).
- **Turret pitch in the install.** Of the INI files the install reads: `AllowsPitch = Yes` 103 times, `TurretPitchRate` 115, `FirePitch` 39, `MinPhysicalPitch` 30 (−15, −20 or −75), `NaturalTurretPitch` 28 (1 or 45), `GroundUnitPitch` 10 (0, 20 or 40), `TurretArtAngle` 24 and `TurretArtPitch` none; `ShotsPerBarrel` 6 times, each 1, its default.
- **A pristine bone is animated (found the same day).** `ModelConditionInfo::validateCachedBones` places a state's bones as its first animation places them at frame 0, or at its last frame with `PRISTINE_BONE_POS_IN_FINAL_FRAME`, which 229 lines of the INI files set. The Tank Hunter's `nimsst_skn` holds its `bone_gun` at (−1.96, 10.86, 10.38) of its frame in its hierarchy's own pose, 10 units to its side, where its default state's first animation, at frame 0, holds its launcher's `muzzle01` 13 units up and 1.7 to its right, 6 forward; launched from the hierarchy's pose, and along its idle facing, as the engine turned no attacker toward its target, the Zero Hour check's Tank Hunter killed no dozer.
