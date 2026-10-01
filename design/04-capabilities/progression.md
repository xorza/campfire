# Progression

## Mechanism

How units grow during a match or a life. `stats` owns a unit's level, as an input to its stats; `progression` decides when it changes, and when a unit may learn a rank of an action.

## Data

The mode's `levels`: the experience each level needs, from level 2, ascending: `levels = [280, 660, ...]`. Its slot kinds' `ranks` and `levels`: the level each rank of a slot's action needs ([Actions](actions.md#data)).

## Rules

- **Experience.** `ctx.add_xp(unit, amount)` adds experience, state beside the level, to any unit: a hero's, an RTS unit's veterancy, an MMO character's. Each level reached raises the unit's level, and with it its stats and, by the rule of a maximum that rises, its pools ([Pools](stats.md#pools)).
- **Learning.** Each level reached gives a unit a point to learn a rank. A player learns by an order, `learn = slot`, which takes a point and a rank its slot kind's `levels` allow. `ctx.learn` grants a rank with no point and no level rule.

## State and derived

- **State:** each unit's experience and unspent points.

## Script API

`ctx.add_xp(unit, amount)`, `ctx.learn(unit, slot)`; `unit.xp`, `unit.points`.

## Network

Experience and points go to the unit's owner; the level to everyone who sees the unit.

## Cost

A level reached costs a refresh of its unit.

## Later

- **Talents:** choices that grant modifiers or actions, as data.
- **Veterancy:** the same levels, driven by kill credit through the mode's script.

Persistent progress, an MMO character's level, is saved by `persistence`; this capability only runs it.

## Genres

A MOBA's levels 1 to 18 and ability ranks; an RTS's veterancy; an MMO's levels and talents. A shooter and a battle royale take none, or a mode's own perks as modifiers.
