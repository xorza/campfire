# Progression

How units grow during a match or a life. `stats` owns a unit's level, as an input to its stats; `progression` decides when it changes.

## Experience and levels

- The mode's `levels` lists the experience each level needs, from level 2: `levels = [280, 660, ...]`, ascending. `ctx.add_xp(unit, amount)` adds experience, state beside the level; each level reached raises the unit's level, and with it its stats and, by the pool rule, its pools ([Stats](stats.md#pools)).
- Each level reached gives an avatar a point to learn a rank. A player learns by an order, `learn = slot`, which takes a point and a rank the slot's level rule allows; the mode's `rank_levels` sets it by slot kind, the reference MOBA's `basic = [1, 3, 5, 7, 9]` and `ultimate = [6, 11, 16]`. `ctx.learn` still grants a rank with no point and no rule.

## Later

- **Talents:** choices that grant modifiers or abilities, as data.
- **Veterancy:** kills raise a unit's rank, as in RTS games; the same levels, driven by kill credit.

Persistent progress (an MMO character's level) is saved by `persistence`; this capability only runs it.
