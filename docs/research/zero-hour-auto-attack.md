# Zero Hour automatic attack

How a Zero Hour unit attacks with no order, read on 21 October 2026 from GeneralsX's `GeneralsMD` source at `0e79905d1` and the shipped INI of `INIZH.big`; and how OpenRA's `AutoTarget` does it, for comparison. Paths are under `GeneralsMD/Code/GameEngine/`.

## The scan

- **Who scans.** An object's AI module reads `AutoAcquireEnemiesWhenIdle`, a bit set of `Yes`, `Stealthed`, `No`, `NotWhileAttacking` and `ATTACK_BUILDINGS` (`AIUpdate.h`, `AutoAcquireStates`); with no `Yes` bit, `getNextMoodTarget` finds nothing for an idle unit, and the field's default is none (`AIUpdateModuleData`'s constructor). A unit that cannot attack, as a dozer of `Yes` with no weapon, finds nothing (`AI::findClosestEnemy`, `CAN_ATTACK`).
- **When.** The idle state and an idle turret ask `getNextMoodTarget` each frame they wake (`AIIdleState::update`, `TurretAI::friend_checkForIdleMoodTarget`); it answers only from `m_nextMoodCheckTime` on, which it then sets `MoodAttackCheckRate` later, 2 s by default (`LOGICFRAMES_PER_SECOND * 2`). Entering the idle state sets the next check `ForceIdleMSEC` later, 67 ms in `AIData.ini`, and offsets the first check by a random value within half the rate (`resetNextMoodCheckTime`). `MoodAttackCheckRate` reads in milliseconds and rounds up to frames (`INI::parseDurationUnsignedInt`), so 250 ms is 8 frames.
- **How far, for a human player.** The range is the object's `VisionRange` times `GuardOuterModifierHuman`, 2.2 (`AI::getAdjustedVisionRangeForObject`); a unit a human player controls then keeps only the targets within the range of one of its weapons (`WITHIN_ATTACK_RANGE`, `PartitionFilterWithinAttackRange`) and clear of its player's fog (`UNFOGGED`), as "our AI will not allow us to pursue the target".
- **How far, for a computer player.** The vision range times `GuardOuterModifierAI`, 1.333, and by mood 1.1 when alert or 1.5 when aggressive; no weapon range or fog filter, so its units pursue.
- **Which target.** With no script's attack priorities, the closest that passes every filter, by 2D bounding-sphere distance (`ThePartitionManager->getClosestObject`, `FROM_BOUNDINGSPHERE_2D`): an enemy of the unit (`PartitionFilterLiveMapEnemies`), alive and on the map, not stealthed and undetected, and, with `AttackUsesLineOfSight = Yes` in `AIData.ini`, in line of sight for an object of `ATTACK_NEEDS_LINE_OF_SIGHT`.
- **Buildings.** Without `ATTACK_BUILDINGS`, a human player's unit rejects every enemy structure but one of `FS_BASE_DEFENSE` and a garrisoned one that can attack (`PartitionFilterRejectBuildings`); a computer player's takes every enemy structure. With `AttackIgnoreInsignificantBuildings = Yes`, a structure of no faction is rejected unless it holds a garrison (`PartitionFilterInsignificantBuildings`).
- **The neutral player.** Every player regards the neutral player as `NEUTRAL` (`PlayerList::newGame`: `setPlayerRelationship(getNeutralPlayer(), NEUTRAL)`), so no scan finds an object the map places for it.

## The attack it starts

- **No pursuit for a human player.** A target the scan found gives an attack of `CMD_FROM_AI`; a human player's unit in such an attack fails out of its pursuit once the target leaves the weapon's range, unless its weapon is a contact weapon, and goes idle again (`AIAttackApproachTargetState::onEnter`, `AIAttackPursueTargetState::onEnter`).
- **A turret scans on its own.** An idle turret, as a base defense's or a moving tank's, takes the scan's target for its own (`TurretAI::friend_checkForIdleMoodTarget`, `m_targetWasSetByIdleMood`), and drops it once out of range.
- **Retaliation.** A human player's unit that takes damage from an enemy within `MaxRetaliationDistance`, 200, that is not airborne, makes each idle, mobile, armed ally within `RetaliationFriendsRadius`, 120, plus its bounding radius, guard-retaliate against it, which pursues (`ActiveBody::attemptDamage`, `shouldRetaliateAgainstAggressor`, `shouldRetaliate`), when the player's retaliation mode is on.

## The shipped data

- **`AIData.ini`.** The root's file is empty, and `Default/AIData.ini` holds the values: `GuardOuterModifierHuman = 2.2`, `GuardOuterModifierAI = 1.333`, `AlertRangeModifier = 1.1`, `AggressiveRangeModifier = 1.5`, `ForceIdleMSEC = 67`, `AttackUsesLineOfSight = Yes`, `AttackIgnoreInsignificantBuildings = Yes`, `MaxRetaliationDistance = 200.0`, `RetaliationFriendsRadius = 120.0`.
- **The flags.** Of the objects' `AIUpdateInterface` modules, 221 say `Yes`, 27 `Yes Stealthed`, 6 `Yes Stealthed ATTACK_BUILDINGS`, 52 `No`, 17 `No NotWhileAttacking`, 13 `No ATTACK_BUILDINGS`, with or without `Stealthed`, and 174 nothing; of the other AI modules, one `JetAIUpdate` says `Yes ATTACK_BUILDINGS`. Dozers and workers say `Yes` and carry no weapon; supply trucks and Chinooks say nothing.
- **The rates.** 50 objects set `MoodAttackCheckRate = 250` and 4 `500`; the others take 2 s.
- **Base defenses.** Of `FS_BASE_DEFENSE`: `ChinaGattlingCannon`, `AmericaPatriotBattery` and `AmericaFireBase` say `Yes` at 250 ms; the tunnel networks `Yes Stealthed`; the Stinger sites, the bunker and the speaker tower have no such AI module. The Gattling Cannon's `GattlingBuildingGun` fires at ground units within 225 every 250 ms with no projectile, and needs `ChinaPowerPlant`; the Fire Base's `FireBaseHowitzerGun` has a `MinimumAttackRange` of 50, and the Patriot fires missiles.

## OpenRA

- **Stances.** `AutoTarget` holds `UnitStance { HoldFire, ReturnFire, Defend, AttackAnything }`, `InitialStance = Defend` for human players and `InitialStanceAI = AttackAnything` for computer players (`OpenRA.Mods.Common/Traits/AutoTarget.cs`, `bleed`).
- **When and how far.** An idle unit of `Defend` or above scans every `MinimumScanTimeInterval` 3 to `MaximumScanTimeInterval` 8 ticks, drawn on the shared random, within `ScanRadius`, its armaments' maximum range by default.
- **Which target.** The highest `AutoTargetPriority` whose `ValidTargets`, `InvalidTargets` and relationships take the target, then the closest; one out of an armament's range only when it may move.
- **Pursuit.** Only `AttackAnything` with `AllowMovement` moves after a target; `ReturnFire` and `Defend` attack without moving.
- **Return fire.** Damage makes an idle unit of `ReturnFire` or above attack its attacker, not one that heals or appears friendly, and one above `ReturnFire` scans again for a better target.
