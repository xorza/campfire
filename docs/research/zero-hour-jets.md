# Zero Hour's jets

How Zero Hour flies a jet, parks it, sends it down a runway, makes it attack and brings it home to reload, and what OpenRA and the engine hold of it, read on 27 October 2026 from [GeneralsX](https://github.com/fbraz3/GeneralsX) (`JetAIUpdate.cpp`, `ParkingPlaceBehavior.cpp`, `AIStates.cpp`, `Locomotor.cpp`), the install's `INIZH.big` (`Locomotor.ini`, `Weapon.ini`, `Object/*.ini`) and models, OpenRA's `FlyAttack.cs` and `ReturnToBase.cs`, and the engine's code.

## What flies on wings

- **Wings.** Of the 148 objects whose `SET_NORMAL` locomotor has `Surfaces = AIR`, 68 have `Appearance = WINGS`, of 22 locomotors.
- **The jets.** `JetAIUpdate` runs the Raptor, the Aurora and the Stealth Fighter of America and the MiG of China, each with its generals' copies, and the Comanche, whose locomotor hovers; the carrier's Raptor and the cinematic copies are not trained.
- **Their locomotors.** The Raptor's flies at 175 a second, at least 60 (`MinSpeed`), turns 120° a second, at a `PreferredHeight` of 100, an air target past 30, with a `Lift` of 120 and a `CirclingRadius` of 100; the MiG's at 160, at least 60, turns 120°, the same height; each has a taxi locomotor, `BasicJetTaxiLocomotor`, on the ground at 50 a second turning 180°.
- **A jet never turns in place.** A unit whose locomotor's `MinSpeed` is above 0 cannot turn in place: as it aims, it keeps the victim's place as its locomotor's goal and fires once the victim lies within its weapon's `AcceptableAimDelta`, at least 0.035 radians, of its facing (`AIAttackAimAtTargetState`, `m_canTurnInPlace`).
- **Wings fly toward their goal.** `Locomotor::moveTowardsPositionWings` moves the jet by the general motion toward its goal, turning at its turn rate while it flies at no less than its minimum speed; its circling for landing is compiled out, as nothing defines `CIRCLE_FOR_LANDING`.
- **Wings circle where they hold.** A jet in the air with no goal flies at its minimum speed toward the point of a circle of its `CirclingRadius` round the place it holds, 157.5° round from its bearing to that place, so it circles it (`Locomotor::maintainCurrentPositionWings`).

## Weapons that reload at home

- **Clips that return to base.** 13 weapons have `AutoReloadsClip = RETURN_TO_BASE`, among them the Raptor's `RaptorJetMissileWeapon`, 4 shots 150 ms apart of 100 each, a range of 320, at least 100, an aim of 30°, a reload of 8,000 ms, and the MiG's `NapalmMissileWeapon`, 2 shots 300 ms apart of 75 with 40 within its secondary radius, a range of 320, at least 80, an aim of 30°, a reload of 8,000 ms.
- **Out of ammunition.** A jet is out of its special ammunition when it has at least one weapon that returns to base and every such weapon is empty (`JetAIUpdate::isOutOfSpecialReloadAmmo`); idle or reloading in the air so, it returns for landing.
- **Idle at home.** A jet idle in the air for its `ReturnToBaseIdleTime`, 10,000 ms for the Raptor and the MiG, returns to its airfield even with ammunition left.
- **The reload.** At its place, each weapon reloads over its `ClipReloadTime` times the share of its clip it lacks, the longest of them the reload's time, its clip filling in proportion as it goes (`JetOrHeliReloadAmmoState`).
- **A dead airfield.** A jet whose airfield is dead flies to where it stood and circles there, taking `OutOfAmmoDamagePerSecond`, 10% of its life for the Raptor and the MiG, each second, as `UNRESISTABLE` damage of no source, and each second looks for an airfield of its player with a free place, which becomes its own (`JetOrHeliCirclingDeadAirfieldState`). An airfield that dies kills each jet on its ground and each one taking off or landing, from its wait for a runway to land (`ParkingPlaceBehavior::killAllParkedUnits`).
- **Commands on the way.** A command a jet takes while it takes off or lands, from its wait for a runway to its place, is held and runs after its reload; one it takes on its way home runs at once, and a jet with no shot left goes home again. A jet sent home empty resumes its last command after its reload only for a guard or a hunt; an attack or a move it drops (`JetAIUpdate::aiDoCommand`, `ALLOW_INTERRUPT_AND_RESUME_OF_CUR_STATE_FOR_RELOAD`, `HAS_PENDING_COMMAND`).
- **The heal.** A jet heals by its airfield's `HealAmountPerSecond`, in steps of a fifth of a second, while it may not fly: from its taxi off the runway as it lands to its roll as it takes off (`ParkingPlaceBehavior::update`, `setHealee`).

## Parking and runways

- **The airfield.** `ParkingPlaceBehavior` gives `NumRows` × `NumCols` places, 2 × 2 at every airfield, an `ApproachHeight` of 50, `HasRunways = Yes`, and a `HealAmountPerSecond` of 10 for a jet parked; China's airfields `ParkInHangars`.
- **A place's bones.** Each place of column `c` and row `r` has the bones `Runway<c>Parking<r>`, its place and facing, `Runway<c>Prep<r>`, where it waits for its runway, and `Runway<c>Park<r>Han`, its hangar; each column's runway runs from `RunwayStart<c>` to `RunwayEnd<c>` (`ParkingPlaceBehavior::buildInfo`). The American and the Chinese airfields' models, `ABArFrcCmd` and `NBAirfield`, hold every one.
- **A place for each jet.** An airfield trains a jet only while a place is free, and reserves one as the train is queued; the jet is made at its place, or at its hangar for an airfield that parks in hangars, and holds that place while it lives (`hasAvailableSpaceFor`, `reserveDoorForExit`, `exitObjectViaDoor`).
- **Taking off.** A jet waits for its column's runway, taxis to its prep point, waits `TakeoffPause`, 500 ms, rolls from the runway's start to its end and climbs on (`TAKING_OFF_AWAIT_CLEARANCE`, `TAXI_TO_TAKEOFF`, `PAUSE_BEFORE_TAKEOFF`, `TAKING_OFF`).
- **Landing.** A jet waits for its runway, flies to an approach point beyond the runway's end at the airfield's `ApproachHeight`, lands from the runway's end to its start, taxis to its place, turns to the place's facing and reloads (`LANDING_AWAIT_CLEARANCE`, `LANDING`, `TAXI_FROM_LANDING`, `ORIENT_FOR_PARKING_PLACE`, `RELOAD_AMMO`).
- **One jet a runway.** A runway is reserved by one jet at a time, for its takeoff or its landing, and the next jet in line for takeoff takes it as the first leaves.

## Doors, takeoff and the approach

Read on 28 October 2026 from GeneralsX (`ProductionUpdate.cpp`, `ParkingPlaceBehavior.cpp`, `JetAIUpdate.cpp`) and the release game's `INIZH.big` (`FactionBuilding.ini`).

- **A door opens before its unit leaves.** A `ProductionUpdate` of `NumDoorAnimations` above 0 opens the door it reserves once a production ends, `DOOR_<n>_OPENING` for `DoorOpeningTime`, makes the unit only once the door waits open, `DOOR_<n>_WAITING_OPEN` for `DoorWaitOpenTime`, each later unit through that door holding it open, then closes it, `DOOR_<n>_CLOSING` for `DoorCloseTime` (`ProductionUpdate::update`). Both airfields give 4 doors, 2,000 ms opening, 3,000 ms open and 2,000 ms closing; America's War Factory 1 door, 3,250, 3,000 and 4,000 ms.
- **An airfield's door is its place.** `ParkingPlaceBehavior::reserveDoorForExit` gives the place it reserves as the door, each place its door in turn, by row then column (`buildInfo`). Every airfield makes its jet in its place's hangar, `Runway<c>Park<r>Han`, and sends it along an exit path to its parking space: its `Runway<c>Parking<r>`, America's, or the hangar itself with `ParkInHangars`, China's (`exitObjectViaDoor`, `calcPPInfo`). Both airfields' models, `ABArFrcCmd` and `NBAirfield`, have both bones for each place.
- **A jet holds its door open.** A place's door is held open while a jet holds the place, from its making or its arrival from another airfield to its death or its release (`reserveSpace`, `releaseSpace`); a held door that is closed begins to open (`ProductionUpdate::setHoldDoorOpen`).
- **A place is reserved as the train queues.** A train at an airfield of no free place is refused as it is queued, a queued train holding its place (`ProductionUpdate::queueCreateUnit`, `reserveDoorForExit`), so an airfield of 4 places never trains a fifth jet while 4 live or wait.
- **A takeoff's path and lift.** A jet takes off by its normal locomotor, of `WINGS`, with its lift set to 0, along a path to the runway's end raised to the approach height, then to the runway's exit; each frame its lift is its locomotor's most times `(1 − d / L)²`, `d` its distance to the runway's end and `L` the runway's length, so it lifts more as it nears the end (`JetTakeoffOrLandingState::update`).
- **The approach and the exit.** Both lie past the runway's end by three quarters of the runway's length, at the runway end's height plus the airfield's `ApproachHeight`, 50 at both airfields (`ParkingPlaceBehavior::calcPPInfo`); a landing jet flies to the approach at its least speed, then down to the runway's end and along it to its start.

## Other engines

- **OpenRA's attack runs.** A fixed-wing aircraft attacks by runs: it flies to its target's center, passes it, flies out to its weapons' range and comes round for the next run, and between them moves within its minimum and maximum range ([FlyAttack.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Activities/Air/FlyAttack.cs)).
- **OpenRA's return to base.** An aircraft out of ammunition returns to the nearest of its owner's free rearming buildings, waits by a busy one, circling for a fixed-wing one, flies idle with none, lands vertically onto its pad and rearms there ([ReturnToBase.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Activities/Air/ReturnToBase.cs)).

## What the engine holds

- **Fliers.** A unit type's `flight` gives a height, a climb and an airborne height, and its walk is planar on the air layer; a walker that faces away from its leg turns before it steps (`walking.rs:477`).
- **Aim.** A weapon no turret carries turns its attacker in place toward a target in its reach and starts within its `aim_deg` (`attacks.rs:308`).
- **Clips.** A clip reloads by time after its last shot (`attacks.rs:626`).
- **Exits.** A producer's `production` gives one exit, a helipad's for an airfield (`production_data.rs:22`).
- **The import.** It drops a train of a unit that does not walk, the jets' among them (`production_import/mod.rs:376`), and moves a type by its first normal locomotor that drives or hovers (`imported_type/mod.rs:153`).
