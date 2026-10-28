# Progression

## Mechanism

- **Tracks of experience, each with its levels.** A MOBA hero has one track; a Skyrim character one for each skill, whose level-ups feed the character's level. `stats` owns a unit's level, and `progression` decides when it changes.

## Rules

- **A level never falls.** A track's level is the highest its experience reached.
- **Level-ups chain within the tick.** Each level reached runs `on_level_up`, which may add experience to another track, as Skyrim's skills feed its character level; a level it reaches runs in the same stage, so a chain ends within the tick.
- **A point for each level,** one as it spawns, so 18 levels give 18 points, as in League of Legends and Dota 2.
- **Learning in Inputs.** A learn applies in input order, so a cast ordered after it in the tick starts at the new rank; a dead unit learns too.
- **Levels and points are state,** not derived, as `ctx.learn` grants a rank with neither.
- **A type's own thresholds.** A type's entry in `tracks` may give its own `levels`, of the count of the track's, as Zero Hour's `ExperienceRequired` and OpenRA's `GainsExperience` are each type's; the track's own are the default. Refused: a track for each list of thresholds, 15 tracks for one veterancy.
- **A level holds a modifier.** A track's `modifiers` name one modifier for each level from 2, which its unit holds while at that level, as a passive is held, as `Object::onVeterancyLevelChanged` sets one bonus condition and clears the others and OpenRA's `GainsExperience` grants a condition for each level. Refused: an `on_level_up` that applies them, a script whose state undoes the level before; stats' `per_level`, which grows in equal steps where the health bonus goes 120 %, 130 %, 150 %.
- **A track of players.** A track of `holder = "player"` keeps its experience and its points on each player's entity, sent to its owner, and a share `to = "player"` gives a death's experience to its killer's player, as OpenRA's `GivesExperience` feeds the killer's player's `PlayerExperience` and Zero Hour's `addSkillPointsForKill` its general. Refused: a hidden unit for the general, which a kill or a script can reach; a player resource, which a cost spends and a bounty pays.
- **A track gives points by level.** A track's `points` give the points of each level from 1, one each by default, as a MOBA's hero gains one a level and Zero Hour's `Rank.ini` grants 1, 1, 1, 1 and 3; the veterancy gives none.
- **A science is a perk of a player.** The mode's `[perks.<name>]` gives each its cost in points, the perks it requires, the level of its track it needs and the modifier its player holds once it has it; a player learns one by an order of no units, applied in Inputs in input order, and a mode's script grants one with `grant_perk(player, id)`, as Zero Hour's `attemptToPurchaseScience` and `grantScience`. A perk of no cost is granted alone, as a science of cost 0; a rank's science is the level it needs; a faction's science is a perk the mode's start grants. Refused: a research of production, which queues at a producer, pays in resources and takes its time; the planned perks of units, which wait for their slice.
- **A perk's modifier is a player modifier.** So a train's `requires.modifiers`, an action's `when` by the tag it grants and a stat it raises read it by the paths they have, as a science turns on Zero Hour's units, powers and starting levels alike; the science prerequisites the import drops convert.
- **A start level by a modifier.** A type's entry in `tracks` may give `starts = [{ level, requires }]`: its unit spawns at the highest of those levels that hold, with its experience at that level's threshold, each always or while its player holds the modifier `requires`, as each `VeterancyGainCreate` module, which may name no science, sets a minimum level, and OpenRA's `ProducibleWithLevel` starts a unit at a level. Refused: one start of a modifier, under which the Tank General's Battlemaster, veteran always and elite by its science, does not convert.

## Planned

- **Perks** grant modifiers and actions, each unit type listing those it may take, as Dota 2 gives each hero its talents, with one perk of a group, as its talent pairs.

## Network

- **A client predicts its own points and the ranks they learn.**
