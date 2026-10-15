# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 8, Zero Hour combat.** The user starts Tournament Desert in the client against an idle server bot in slot 2, trains rangers, orders them to attack player 2's command center, and destroys it; player 2's dozer dies with it, and the match ends with the user as the winner. The design is [Zero Hour combat](docs/design/19-zero-hour-combat.md); the facts are [Zero Hour research](docs/research/zero-hour.md#combat). The Zero Hour check plays the fight to its end.

## 1. Zero Hour fights

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`.

### C5. The import of teams, life, weapons and armor

(`import`; design 19, D1, D5, D6.) The eight teams; each object's life and body by its `Body`; the damage kinds and the armors of `Armor.ini`; the weapons of each type's starting set, converted or dropped by cause; the tag `counts_for_victory`. Check: design 19's import checks.

### C6. The skirmish's start and end

(`source/packages/zero-hour`; design 19, D1, D7.) The rules package's mode script places a start for each held slot alone, and in `on_unit_died` defeats a player at its last building that counts for victory, kills its units, and ends the match at the last team in play. Check: a scripted match of the imported fixture mode where one player's building dies, its other units die in the next tick, and the match ends with the other team the winner.

### C7. The Zero Hour check fights

(`checks/zero-hour-check`; design 19, Checks.) The counts of weapons converted and dropped by cause, of armors and of damage kinds; on Tournament Desert, against an idle second slot, player 1's rangers destroy player 2's command center in their pinned ticks, its dozer dies in the next tick, and the match ends with `player_1` the winner. Check: the check passes on Linux and on macOS, with the fingerprints pinned again from both.
