# Zero Hour's ground locomotors

What the released source (GeneralsX, read as a specification) and the install's `INIZH.big` say of how a unit on the ground speeds up, brakes and turns along its path. Read 28 October 2026. Paths are under `GeneralsMD/Code/GameEngine/`, and `Core/GameEngine/` for `GameDefines.h` and `INI.cpp`.

## The data

- **182 locomotors.** `Locomotor.ini`'s `Appearance` lines: `FOUR_WHEELS` 43, `TREADS` 33, `TWO_LEGS` 28, `THRUST` 26, `WINGS` 22, `HOVER` 20, `MOTORCYCLE` 6, `CLIMBER` 3.
- **Units.** `Speed`, `MinSpeed` and `MinTurnSpeed` read in units a second and become units a frame (`INI::parseVelocityReal`); `Acceleration` and `Braking` in units a second squared, a frame squared (`parseAccelerationReal`); `TurnRate` in degrees a second, radians a frame (`parseAngularVelocityReal`).
- **Samples.** `CrusaderLocomotor`: `TREADS`, `Speed = 30`, `SpeedDamaged = 25`, `TurnRate = 180`, `Acceleration = 1000`, `Braking = 1000`, `MinTurnSpeed = 0`. `HumveeLocomotor`: `FOUR_WHEELS`, `Speed = 60`, `SpeedDamaged = 30`, `TurnRate = 180`, `Acceleration = 1000`, `Braking = 1000`, `MinTurnSpeed = 20`, `TurnPivotOffset = -0.33`, `CanMoveBackwards = Yes`. `BasicHumanLocomotor`: `TWO_LEGS`, `Speed = 20`, `SpeedDamaged = 10`, `TurnRate = 500`, `Acceleration = 100`, `Braking = 100`.
- **Acceleration and braking of the ground movers.** Of `TREADS`, `FOUR_WHEELS`, `MOTORCYCLE`, `TWO_LEGS` and `CLIMBER`: 21 legs of 100 and 100; 19 treads and 9 wheels of 1,000 and 1,000; 7 treads and 7 wheels of 30 and 50; 5 wheels of 10 and 50; the rest spread from 5 to 240. `OverlordLocomotor` gives `Speed = 20` and `Acceleration = 15`, `AmericaVehicleDozerLocomotor` 30 and 30, `RocketBuggyLocomotor` 90 and 90, `TechnicalLocomotor` 90 and 100.
- **Pivots and reversing.** 51 locomotors give a `TurnPivotOffset`, 29 of them `-0.5` and 11 `-0.33`; 19 say `CanMoveBackwards = Yes`.
- **A damaged mover.** `getMaxSpeedForCondition`, `getMaxTurnRate` and `getMaxAcceleration` take the `Damaged` values once the body's damage state is no better than `GameData.ini`'s `MovementPenaltyDamageState = REALLYDAMAGED`, which `UnitReallyDamagedThreshold = 0.35` sets.

## The physics

- **One velocity, integrated each frame.** `PhysicsBehavior::update` adds gravity and friction to the frame's acceleration, adds the acceleration to the velocity, zeroes each axis under 0.001, and moves the object by the velocity; a `BRAKING` object moves only in height there, as its locomotor places it.
- **A mover keeps no sideways velocity for long.** `applyFrictionalForces` on the ground damps the velocity across the facing by the lateral friction, 0.15 by default, each frame; a motive object takes no forward friction; `applyForce` of a motive object keeps only the component across the facing, and `applyMotiveForce` the rest.
- **The forward speed of the release game.** `getForwardSpeed2D` returns `±√((vₓdₓ)² + (v_yd_y)²)`, which understates the speed by up to `1/√2` on a diagonal heading, so a mover that drives to its goal speed by it drives up to `√2` times as fast on a diagonal. TheSuperHackers' fix returns the dot product and scales the commanded speeds by `4 − 2√2`, 1.1716; `PRESERVE_RETAIL_PHYSICS_FORWARD_SPEED_DISCREPANCY` is 1 by default, so the build keeps the release game's.

## The movers

- **Treads and legs slow with their turn.** `moveTowardsPositionTreads` and `moveTowardsPositionLegs` turn the object toward its goal by at most its turn rate a frame, about its pivot (`rotateObjAroundLocoPivot`), and set its goal speed to its desired speed times `1 − min(|relAngle| / 45°, 1)`, so a mover more than 45° off its goal turns in place.
- **Treads brake toward the end.** The slow-down distance is `speed / 1.5 × speed / Braking`; within it the locomotor brakes, by `Braking` once past the distance and by half within three quarters of it, its braking factor `(slowDown / dist)²` up to a maximum, and within two cells of the goal, more than 2.25° off it, its goal speed is 60 % of its speed. The force toward the goal speed is at most `mass × Acceleration` speeding up and `mass × factor × Braking` slowing down, and no more than the change needs.
- **Legs stop at their own distance.** Within `calcSlowDownDist` of their goal, legs take `MinSpeed` as their goal speed; a `WanderWidthFactor` sways their heading.
- **Wheels turn at a turning speed.** `moveTowardsPositionWheels`, for `FOUR_WHEELS` and `MOTORCYCLE`, holds its desired speed to its turning speed, `MinTurnSpeed` and at least a quarter of `Speed`, while it turns more than 9°; from a stop with its goal more than 90° behind, a mover of `CanMoveBackwards` backs toward it, and turns in three points when its goal lies more than 5 major radii away; at more than 15° it projects its arc half a second ahead and turns in place with no force when the half or the end of the arc is no valid ground; its slow-down distance is `speed / 1.5 × (speed / Braking + 1) + speed`, at least one cell.
- **A pivot moves the body.** With a `TurnPivotOffset`, a turn rotates the object about the point that offset times its bounding radius ahead of its center, so a car of `-0.33` swings its front round its rear axle; while braking it turns about its center.
- **A braking mover lands on its goal.** As `IS_BRAKING` holds, `locoUpdate_moveTowardsPosition` moves the object straight toward its goal by its forward speed, at least a cell a second and no farther than the goal, instead of by its velocity.

## Other engines

- **OpenRA.** A `Mobile` actor moves at its speed from its first tick, with no acceleration, and turns by its `TurnSpeed`.
- **Spring.** A unit definition gives a ground unit `maxAcc`, `maxDec` and `turnRate`, by which its `GroundMoveType` speeds up, slows down and turns as it drives.
