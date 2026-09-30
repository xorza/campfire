# Progression

How units grow during a match or a life.

- **Experience and levels:** a level curve in data; `ctx.add_xp` gives experience, and a level-up raises the unit's per-level stats.
- **Learning:** points to spend on ranks of abilities (a MOBA's skill points) or on talents; `ctx.learn` lets a mode grant ranks directly.
- **Talents:** choices that grant modifiers or abilities, as data.
- **Veterancy:** kills raise a unit's rank, as in RTS games; the same levels, driven by kill credit.

Persistent progress (an MMO character's level) is saved by `persistence`; this capability only runs it.
