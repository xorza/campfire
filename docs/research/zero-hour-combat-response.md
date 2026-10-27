# Zero Hour's retaliation, turret scan and weapon choice

What the released source (GeneralsX, read as a specification) and the install's `INIZH.big` say of how a unit answers fire, how its turret scans while it drives, and which weapon it fires at a target, beside what [Zero Hour automatic attack](zero-hour-auto-attack.md) gives of the idle scan. Read 28 October 2026. Paths are under `GeneralsMD/Code/GameEngine/`.

## Retaliation

- **Who calls for it.** `ActiveBody::attemptDamage` calls each time a unit takes damage, dead or not, when its controlling player is human and its player's retaliation mode is on (`Player::isLogicalRetaliationModeEnabled`, which the client's option `Retaliation`, on by default, sets each second by a message). `shouldRetaliateAgainstAggressor` refuses no damager, an airborne one, one not `ENEMIES` to the unit, one farther than `MaxRetaliationDistance`, 200, by 2D bounding spheres, a unit of a player not human, and a drone.
- **Who answers.** Every object of the unit's player's allies, its own included, on the map, whose center lies within `RetaliationFriendsRadius`, 120, plus the unit's bounding circle's radius, on the ground (`FROM_CENTER_2D`), that `shouldRetaliate` takes: not `CANNOT_RETALIATE`, not `IMMOBILE`, not a drone, with an AI that is idle, not stealthed and undetected, not using an ability; and that may attack the damager now or after moving (`getAbleToAttackSpecificObject`). Each takes `aiGuardRetaliate(damager, its position)`, a command from the AI.
- **The chase.** The guard-retaliate machine starts by attacking the damager, pursuing it. The attack ends after `GuardChaseUnitsDuration`, 10,000 ms, when the damager is gone, when the damager lies farther from the guard point than the unit's `VisionRange` times `GuardOuterModifierHuman` plus `GuardInnerModifierHuman`, 2.2 plus 1.8, or when the unit lies farther from the point than its `VisionRange` times 1.8, each on the ground (`GuardRetaliateExitConditions::shouldExit`).
- **The return.** The unit then moves back to the point, looking every `GuardEnemyReturnScanRate`, 1,000 ms, for an enemy within 1.8 times its vision range, not a structure but a base defense, which it attacks within 1.5 times that range of the point; at the point it looks every `GuardEnemyScanRate`, 500 ms, and leaves the machine when it finds none (`AIGuardRetaliateReturnState`, `AIGuardRetaliateIdleState`, `lookForInnerTarget`).
- **The vision ranges.** Of the objects' `VisionRange` lines, by value, 225 give 200, 224 give 150, 119 give 300, 93 give 0, 54 give 100 and 49 give 180; a GLA vehicle of `GLAVehicle.ini` gives 125.
- **A computer player's units do not retaliate.** Their idle scan reaches the vision range times `GuardOuterModifierAI`, 1.333, with no weapon-range or fog filter, and pursues (zero-hour-auto-attack.md, The scan).

## A turret on the move

- **A turret scans while it is idle.** A turret's machine stays in `TurretAIIdleState` while it has no target, whatever its unit does, and each wake calls `TurretAI::friend_checkForIdleMoodTarget`, which takes `getNextMoodTarget(true, true)`, the idle scan's target, for the turret alone (`setTurretTargetObject`, `m_targetWasSetByIdleMood`), and chooses its unit's best weapon for it; so a tank driving under a move order turns its turret onto an enemy its scan finds and fires, and drives on.
- **The scan's tests.** `getNextMoodTarget` answers nothing for a unit of no `Yes` bit in `AutoAcquireEnemiesWhenIdle`, one attacking with `NotWhileAttacking`, one stealthed that may not scan while stealthed, and one using an ability, and only every `MoodAttackCheckRate`.
- **The turret drops it out of range,** and goes back to its idle scan.

## Weapon choice

- **When.** `AIAttackState::chooseWeapon` chooses the attacker's weapon for its target each update of its attack, and a turret's idle scan for the target it takes, both by `PREFER_MOST_DAMAGE` (`Object::chooseBestWeaponForTarget`).
- **Which weapons may fire.** `WeaponSet::chooseBestWeaponForTarget` returns at once for a locked weapon and takes the primary for no target. Otherwise it passes over a slot of no weapon, one whose command sources the command does not hold, one out of ammunition that does not reload its clip, one whose anti-mask does not hold the target's, one beyond its pitch, and one whose estimated damage is 0 or less, but for `UNRESISTABLE`.
- **Which it prefers.** It walks the slots from the tertiary to the primary, and keeps each weapon of at least the best damage so far, so of equal damage the lower slot wins; a weapon ready to fire is always taken over one that is not, which counts only when none is ready; a weapon on a turret still aiming at the target counts as not ready (`isWeaponSlotOnTurretAndAimingAtTarget`).
- **The estimate.** `Weapon::estimateWeaponDamage` gives 0 for a weapon out of ammunition that does not reload, else the template's primary damage times the attacker's bonus, as the victim's armor takes it (`Object::estimateDamage`), with set values for shrubbery, a sniper against an empty structure, a garrison-clearing or surrender weapon, disarm and deploy.
- **A preference.** A weapon set's `PreferredAgainst = <slot> <kinds>` makes a weapon count as one of huge damage and range against a target of any of those kinds, ready unless out of ammunition. The install holds 122 such lines: `SECONDARY BALLISTIC_MISSILE AIRCRAFT` 29, `PRIMARY INFANTRY` 28, `SECONDARY INFANTRY` 23, `SECONDARY VEHICLE STRUCTURE` 14, `TERTIARY AIRCRAFT` 9, `TERTIARY BALLISTIC_MISSILE AIRCRAFT` 5, `PRIMARY VEHICLE INFANTRY` 5, `PRIMARY AIRCRAFT` 5, `TERTIARY VEHICLE STRUCTURE` 3, `SECONDARY AIRCRAFT` 1.
- **What it applies to.** Of the imported types, 19 hold two weapons in their starting set and the rest one: Colonel Burton's, the Missile Defender's, the Patriot's and the Comanche's of each general, and the Red Guard's of the campaign.

## OpenRA

- **Return fire is the unit's own.** `AutoTarget` answers damage on the damaged unit alone, by its stance (zero-hour-auto-attack.md, OpenRA).
- **Every armament that can fire does.** `AttackBase` takes each armament valid for the target, and each fires as it is ready, so a unit picks no one weapon.
