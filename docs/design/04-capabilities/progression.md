# Progression

## Mechanism

How units grow during a match or a life: experience on **tracks**, each with its own levels; points to learn ranks of actions; and **perks** that grant modifiers and actions. `stats` owns a unit's level, as an input to its stats; `progression` decides when it changes. A MOBA hero has one track; a Skyrim character a track for each skill, whose level-ups feed the character's level, as Skyrim's skills level by use.

## Data

The mode's `[tracks.<name>]`, at most 32: the experience each level needs, from level 2, positive and strictly ascending (`levels = [280, 660, ...]`), and whether the track is the unit's `level` (`level = true`, one track at most), which its stats read. A unit type lists the tracks it has: `tracks = ["level"]`. The slot kinds' `ranks` and `levels` give the level of the `level` track each rank of an action needs ([Actions](actions.md#data)). A package's `[perks.<id>]`: what it needs (tracks at levels, other perks), its cost in points, and what it grants (modifiers, held as passives, and actions in a slot kind). Perks are planned, for the RPG and MMO modes of stage 9: which unit types may take a perk, and which perks exclude each other, are not decided yet.

## Rules

- **Experience.** `ctx.add_xp(unit, track, amount)`, or the effect `xp = { track, amount }`, adds experience to a track of any unit: a hero's, an RTS unit's veterancy, a Skyrim character's archery when an arrow hits. The amount is a decimal, not negative, so a share splits exactly, and experience stops at the largest number; a unit without the track fails the call. A track's level is the highest it reached, as experience meets each threshold; experience never lowers it. Each level reached on a track runs the mode's `on_level_up(ctx, unit, track, level)`, in the Mode stage after `on_unit_died`, in the order reached, which may add experience to another track: Skyrim's rule, character experience from skill level-ups, is that hook. A level a hook reaches runs in the same stage, so a chain ends within the tick, as levels are finite. A call that finds the mode pool spent waits, with those after it, for a later tick's Mode stage, and runs there first; one whose unit is gone by then does not run. A level reached on the `level` track raises the unit's level, and with it its stats and, by the rule of a maximum that rises, its pools ([Pools](stats.md#pools)).
- **Points.** A unit with the `level` track has a point for each level it has there: one as it spawns, at level 1, and one more for each level it reaches, as League of Legends and Dota 2 give them, so 18 levels give 18 points. A unit keeps the points it does not spend.
- **Learning.** A player learns by the order `learn = slot`: the next rank of the action in the slot, for a point. The order holds when the player controls the unit, the slot's kind has `ranks`, the action is below its last rank, the unit has a point, and the unit's level is at least the one the kind's `levels` give the next rank; otherwise it is dropped, as an order that fails its checks is. A dead unit learns too, as in League of Legends and Dota 2. Learning is not an order of the unit's actions: it stops nothing under way, so a channel or a walk goes on. It applies in Inputs, in the order of the tick's orders, so a cast ordered after it in the same tick starts at the new rank, and a passive holds its new rank from the same stage. `ctx.learn` grants a rank with no point and no level rule; `ctx.grant_perk` grants a perk with no point and no rule.
- **Perks** grant what they name for as long as the unit has them; a perk is never lost unless a script removes it.

## State and derived

- **State:** each unit's experience and level on each track, its unspent points, and its perks. A level is state, not derived from experience, so a script may set it, as `ctx.learn` grants a rank. Points are state, not derived from the level, as `ctx.learn` spends none.

## Script API

`ctx.add_xp(unit, track, amount)`, `ctx.grant_perk(unit, id)`; `unit.xp(track)`, `unit.track_level(track)`, `unit.points`, `unit.has_perk(id)`; the hook `on_level_up(ctx, unit, track, level)`.

## Network

Experience, points and perks go to the unit's owner; the level to everyone who sees the unit. A client predicts its own units' points and the ranks they learn, as it predicts their actions.

## Cost

A level reached costs an `on_level_up` call, and on the `level` track a refresh of its unit.

## Genres

A MOBA's levels 1 to 18 and ability ranks; an RTS's veterancy; an MMO's levels and talents; Skyrim's skills, character level and perk trees; Diablo's levels and skill points. A shooter and a battle royale take none, or a mode's own perks.
