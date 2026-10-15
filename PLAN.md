# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 8, Zero Hour combat.** The user starts Tournament Desert in the client against an idle server bot in slot 2, trains rangers, orders them to attack player 2's command center, and destroys it; player 2's dozer dies with it, and the match ends with the user as the winner. The design is [Zero Hour combat](docs/design/19-zero-hour-combat.md); the facts are [Zero Hour research](docs/research/zero-hour.md#combat). The Zero Hour check plays the fight to its end.

## 1. Predicting worlds as a client's

`capabilities`: the test fixtures. It goes first, as combat adds systems a client runs, and the gap it closes hid the last client panic.

### C1. A predicting test world holds no spawner

(`capabilities`; design 14, D12.) A predicting test world holds a `Spawner` because its fixtures load one as they place units, where a client holds none, so a system that spawns on a client passes the tests (the issue log's **Capabilities** item). The fixtures of a predicting world place their units through a spawner they hold alone, and the world keeps none. Check: a system that asks a predicting world for its spawner fails a test; the goldens do not move.

## 2. Weapons and armor in the engine

`actions`, `combat`, `core` (script API). It needs nothing of the import, and the import's steps need it.

### C2. An attack's own damage, period and clip

(`actions`, `combat`; design 19, D2, D3.) An `attack` action takes `amount` in place of `damage` and `period_ms` in place of `rate`, the load requiring one of each pair, and `clip = { shots, reload_ms }`; the slot's times count the shots its clip has left, predicted. `Weapon` holds its damage and its period as an enum of a stat or a number of each rank. Check: design 19's combat checks of an amount, a period and a clip, on the server and on a predicting client; the load's refusals; the goldens do not move.

### C3. Armor

(`combat`; design 19, D4.) `[combat] armors`, a unit type's `combat.armor`, and the damage pass's multiply before `calc_damage`, resolved at the load. Check: design 19's armor checks; the load's refusals; the goldens do not move.

### C4. The script's held slots, a player's units, and a kill

(`core`, `combat`; design 19, D1, D7.) `ctx.held(player)`, `ctx.units_of(player)` and `ctx.kill(unit)`, each in the registry, so the reference gains them. Check: design 19's core checks; the reference test passes once blessed.

## 3. Zero Hour fights

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`. It needs C2 to C4.

### C5. The import of teams, life, weapons and armor

(`import`; design 19, D1, D5, D6.) The eight teams; each object's life and body by its `Body`; the damage kinds and the armors of `Armor.ini`; the weapons of each type's starting set, converted or dropped by cause; the tag `counts_for_victory`. Check: design 19's import checks.

### C6. The skirmish's start and end

(`source/packages/zero-hour`; design 19, D1, D7.) The rules package's mode script places a start for each held slot alone, and in `on_unit_died` defeats a player at its last building that counts for victory, kills its units, and ends the match at the last team in play. Check: a scripted match of the imported fixture mode where one player's building dies, its other units die in the same tick, and the match ends with the other team the winner.

### C7. The Zero Hour check fights

(`checks/zero-hour-check`; design 19, Checks.) The counts of weapons converted and dropped by cause, of armors and of damage kinds; on Tournament Desert, against an idle second slot, player 1's rangers destroy player 2's command center in their pinned ticks, its dozer dies in the same tick, and the match ends with `player_1` the winner. Check: the check passes on Linux and on macOS, with the fingerprints pinned again from both.
