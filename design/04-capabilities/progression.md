# Progression

## Mechanism

How units grow during a match or a life: experience on **tracks**, each with its own levels; points to learn ranks of actions; and **perks** that grant modifiers and actions. `stats` owns a unit's level, as an input to its stats; `progression` decides when it changes. A MOBA hero has one track; a Skyrim character a track for each skill, whose level-ups feed the character's level, as Skyrim's skills level by use.

## Data

The mode's `[tracks.<name>]`, at most 32: the experience each level needs, from level 2, positive and strictly ascending (`levels = [280, 660, ...]`), and whether the track is the unit's `level` (`level = true`, one track at most), which its stats read. A unit type lists the tracks it has: `tracks = ["level"]`. The slot kinds' `ranks` and `levels` give the level of the `level` track each rank of an action needs ([Actions](actions.md#data)). A package's `[perks.<id>]`: what it needs (tracks at levels, other perks), its cost in points, and what it grants (modifiers, held as passives, and actions in a slot kind).

## Rules

- **Experience.** `ctx.add_xp(unit, track, amount)`, or the effect `xp = { track, amount }`, adds experience to a track of any unit: a hero's, an RTS unit's veterancy, a Skyrim character's archery when an arrow hits. The amount is a decimal, not negative, so a share splits exactly, and experience stops at the largest number; a unit without the track fails the call. A track's level is the highest it reached, as experience meets each threshold; experience never lowers it. Each level reached on a track runs the mode's `on_level_up(ctx, unit, track, level)`, in the Mode stage after `on_unit_died`, in the order reached, which may add experience to another track: Skyrim's rule, character experience from skill level-ups, is that hook. A level a hook reaches runs in the same stage, so a chain ends within the tick, as levels are finite. A level reached on the `level` track raises the unit's level, and with it its stats and, by the rule of a maximum that rises, its pools ([Pools](stats.md#pools)).
- **Learning.** Each level of the `level` track gives the unit a point. A player learns by an order, `learn = slot`, which takes a point and a rank its slot kind's `levels` allow; or `perk = id`, which takes the perk's cost and needs its requirements. `ctx.learn` and `ctx.grant_perk` grant a rank or a perk with no point and no rule.
- **Perks** grant what they name for as long as the unit has them; a perk is never lost unless a script removes it.

## State and derived

- **State:** each unit's experience and level on each track, its unspent points, and its perks. A level is state, not derived from experience, so a script may set it, as `ctx.learn` grants a rank.

## Script API

`ctx.add_xp(unit, track, amount)`, `ctx.grant_perk(unit, id)`; `unit.xp(track)`, `unit.track_level(track)`, `unit.points`, `unit.has_perk(id)`; the hook `on_level_up(ctx, unit, track, level)`.

## Network

Experience, points and perks go to the unit's owner; the level to everyone who sees the unit.

## Cost

A level reached costs an `on_level_up` call, and on the `level` track a refresh of its unit.

## Genres

A MOBA's levels 1 to 18 and ability ranks; an RTS's veterancy; an MMO's levels and talents; Skyrim's skills, character level and perk trees; Diablo's levels and skill points. A shooter and a battle royale take none, or a mode's own perks.
