# Progression

## Mechanism

- **Tracks of experience, each with its levels.** A MOBA hero has one track; a Skyrim character one for each skill, whose level-ups feed the character's level. `stats` owns a unit's level, and `progression` decides when it changes.

## Rules

- **A level never falls.** A track's level is the highest its experience reached.
- **Level-ups chain within the tick.** Each level reached runs `on_level_up`, which may add experience to another track, as Skyrim's skills feed its character level; a level it reaches runs in the same stage, so a chain ends within the tick.
- **A point for each level,** one as it spawns, so 18 levels give 18 points, as in League of Legends and Dota 2.
- **Learning in Inputs.** A learn applies in input order, so a cast ordered after it in the tick starts at the new rank; a dead unit learns too.
- **Levels and points are state,** not derived, as `ctx.learn` grants a rank with neither.

## Planned

- **Perks** grant modifiers and actions, each unit type listing those it may take, as Dota 2 gives each hero its talents, with one perk of a group, as its talent pairs.
- **Tracks as data, planned.** With the slice that needs each: a track's points for each level as its data, and tracks on players as well as on units.

## Network

- **A client predicts its own points and the ranks they learn.**
