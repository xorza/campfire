# Progression

## Mechanism

How units grow during a match or a life: experience on **tracks**, each with its own levels; points to learn ranks of actions; and **perks** that grant modifiers and actions. `stats` owns a unit's level; `progression` decides when it changes. A MOBA hero has one track; a Skyrim character one for each skill, whose level-ups feed the character's level.

## Data

The mode's `[tracks.<name>]`, at most 32: the experience each level needs from level 2, positive and strictly ascending (`levels = [280, 660, ...]`), and whether it is the unit's `level` (`level = true`, one track at most), which its stats read. A unit type lists its tracks: `tracks = ["level"]`. The slot kinds' `levels` give the `level` track's level each rank needs ([Actions](actions.md#data)).

Perks are planned: a package's `[perks.<id>]` with what it needs (tracks at levels, other perks), its cost in points, and what it grants (modifiers held as passives, actions in a slot kind); a unit type lists the perks it may take, as Dota 2 gives each hero its talents; a perk's `group` admits one perk of the group, as Dota 2's talent pairs.

## Rules

- **Experience.** `ctx.add_xp(unit, track, amount)`, or the effect `xp = { track, amount }`, adds a decimal amount, not negative, to a track of any unit, stopping at the largest number; a unit without the track fails the call. A track's level is the highest it reached, as experience meets each threshold, and never falls. Each level reached runs the mode's `on_level_up(ctx, unit, track, level)`, in Mode after `on_unit_died`, in the order reached; it may add experience to another track, as Skyrim's skills feed its character level, and a level it reaches runs in the same stage, so a chain ends within the tick. A call that finds the mode pool spent waits, with those after it, for a later tick, and one whose unit is gone by then does not run. A level of the `level` track raises the unit's level, its stats and, by the rule of a rising maximum, its pools ([Pools](stats.md#pools)).
- **Points.** A unit with the `level` track has a point for each level it has, one as it spawns, so 18 levels give 18 points, as in League of Legends and Dota 2, and keeps those it does not spend.
- **Learning.** `learn = slot` learns the slot's action's next rank for a point, when the player controls the unit, the slot's kind has `ranks`, the action is below its last rank, the unit has a point, and its level is at least the next rank's; otherwise it is dropped. A dead unit learns too. Learning stops nothing under way. It applies in Inputs, in order, so a cast ordered after it in the tick starts at the new rank. `ctx.learn` grants a rank with no point and no level rule; `ctx.grant_perk` a perk with no point and no rule.
- **Perks** grant what they name while the unit has them; a perk is lost only if a script removes it.

## State and derived

- **State:** each unit's experience and level on each track, its unspent points, its perks. Levels and points are state, not derived, as `ctx.learn` grants a rank with neither.

## Script API

`ctx.add_xp(unit, track, amount)`, `ctx.grant_perk(unit, id)`; `unit.xp(track)`, `unit.track_level(track)`, `unit.points`, `unit.has_perk(id)`; the hook `on_level_up(ctx, unit, track, level)`.

## Network

Experience and points go to the unit's owner, or as the mode sets ([Replication audiences](../14-replication-audiences.md), `progression`); the level to everyone who sees the unit. A client predicts its own units' points and the ranks they learn.

## Cost

A level reached costs an `on_level_up` call, and on the `level` track a refresh of its unit.

## Genres

A MOBA's levels and ability ranks; an RTS's veterancy; an MMO's levels and talents; Skyrim's skills and perk trees; Diablo's levels and skill points.
