# Zero Hour's helicopters

How Zero Hour flies a helicopter, makes it a target and gathers with it, and what the engine holds of flight, read on 27 October 2026 from [GeneralsX](https://github.com/fbraz3/GeneralsX) (`Locomotor.cpp`, `AIUpdate.cpp`, `WeaponSet.cpp`, `Weapon.cpp`, `PhysicsUpdate.cpp`, `DockUpdate.cpp`, `ParkingPlaceBehavior.cpp`, `ChinookAIUpdate.cpp`), the install's `INIZH.big` (`Locomotor.ini`, `Object/*.ini`), OpenRA's `Aircraft.cs`, and the engine's code.

## What flies

- **Three kinds of air locomotor.** Of the objects whose `SET_NORMAL` locomotor has `Surfaces = AIR`, 38 have `Appearance = HOVER`, 68 `WINGS` and 42 `THRUST`, the missiles'.
- **The hovering objects.** The Chinook, the Comanche and the Helix, each with its generals' copies, are trained; the battle, scout and point defense drones are spawned by their vehicles; the parachutes, the countermeasure flares and the arrows are neither.
- **Their locomotors.** The Chinook's flies at 150 a second, the Comanche's at 120, the Helix's at 75; each turns 180° a second, holds a `PreferredHeight` of 100 above the surface under it (`ZAxisBehavior = SURFACE_RELATIVE_HEIGHT`), has a `Lift` of 120 and an `AirborneTargetingHeight` of 30, and a taxi locomotor for the ground (`BasicHelicopterTaxiLocomotor`).
- **A height by lift and gravity.** A hovering locomotor accelerates toward its preferred height above the surface at a lift of at most its `Lift` net of gravity and brakes by gravity going up, by the classic equation of motion solved for its acceleration, limited by its `SpeedLimitZ` (`Locomotor::calcLiftToUseAtPt`).
- **A hover tilts.** `ForwardVelocityPitchFactor`, −0.05 for the Comanche and −0.1 for the Chinook, pitches the model by its forward speed, and `LateralVelocityRollFactor`, 0.1, rolls it in a turn, through the damped springs of `PitchStiffness`, `RollStiffness`, `PitchDamping` and `RollDamping` that also tilt a tank.
- **Rotors are an animation.** The Comanche's default state loops its model's animation `AVComanche.AVComanche`.

## A target in the air

- **An airborne target by height.** After its movement each frame, an object whose height above the terrain passes its locomotor's `AirborneTargetingHeight` holds `OBJECT_STATUS_AIRBORNE_TARGET`, and loses it at or below (`AIUpdate.cpp`).
- **A weapon's anti flags pick its victims.** A victim that is an airborne target and a vehicle needs `AntiAirborneVehicle`, one on the ground `AntiGround` (`WeaponSet::getVictimAntiMask`).
- **Range is flat, damage is not.** An attack's range is measured in 2D between bounding circles (`ATTACK_RANGE_IS_2D`, `Weapon.cpp`); its damage in 3D.
- **Fliers push each other.** An object above the terrain meets another by their bounding spheres in 3D, and a collision pushes them apart by a force (`PhysicsBehavior::onCollide`), so helicopters spread and never meet a ground unit 100 below.

## Making and gathering

- **The Chinook.** `PRODUCED_AT_HELIPAD`, trained at the Supply Center for 1,200 in 10 s, whose `SupplyCenterProductionExitUpdate` makes it at the center and sends it to (44, 0, 0); a `ChinookAIUpdate` of `MaxBoxes` 8, `SupplyWarehouseActionDelay` 1,200 ms a box, `SupplyCenterActionDelay` 2,900 ms and `SupplyWarehouseScanDistance` 700; a `TransportContain` of 8 slots.
- **The Comanche.** `PRODUCED_AT_HELIPAD`, trained at the Airfield for 1,500 in 20 s, of 220 life; a `JetAIUpdate` whose turret neither turns nor pitches; a 20 mm cannon, an anti-tank missile and, after its upgrade, rocket pods.
- **The Helix.** China's, trained at its Airfield for 1,500; a `ChinookAIUpdate` of no box, so it gathers nothing; a minigun; a `TransportContain` of 5 slots and the upgrades that build on it.
- **A helipad.** An airfield's `ParkingPlaceBehavior` makes a unit `PRODUCED_AT_HELIPAD` at its model's `HeliPark01` bone, turned as the bone, and sends it to the rally point, or to the helipad (`ParkingPlaceBehavior::exitObjectViaDoor`).
- **A flier docks over its dock.** A dock with no enter bone places a flier at its own center, and the search for a free place round it ignores the dock for a flier, so it hovers right over it (`DockUpdate.cpp`); only a projectile flies to a precise height, so a Chinook keeps its preferred height as it docks.

## Other engines

- **OpenRA.** An `Aircraft` cruises at `CruiseAltitude` and moves its altitude at a fixed `AltitudeVelocity`, with no acceleration field; it grants its `AirborneCondition` while its altitude above the terrain is at least `MinAirborneAltitude`, on which its `Targetable` traits switch between air and ground target types; a `Repulsable` aircraft is pushed apart from others ([Aircraft.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Traits/Air/Aircraft.cs)).
- **StarCraft II.** A unit type is air or ground; only a Terran building that lifts off changes, by its state.

## What the engine holds

- **Layers.** A mode's `[navigation] layers` name its layers, a type's `collision` names the layer its body moves on, each layer's name tags the types on it, and collision and pathing work within a layer; the engine test mode `hooks` declares `ground` and `air`.
- **No flight.** A walker steps toward its route's waypoints in 3D, and a waypoint stands on the ground, so no unit holds a height of its own.
- **The import flies nothing.** It gives no speed to a type whose locomotor does not drive on the ground, drops a train of a unit that does not walk and a gatherer that does not walk, and drops a weapon that is not `AntiGround`; of the 38 hovering objects none is written to move.
