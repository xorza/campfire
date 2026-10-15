# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 5, units that drive.** The user starts Tournament Desert in the client, finds America's command center and dozer at their start, selects the dozer, and drives it round the cliffs to the other start, at its speed and turn rate, on the drawn ground. The design is [Units that drive](docs/design/17-units-that-drive.md); the facts are [Zero Hour research](docs/research/zero-hour.md#movement). The Zero Hour check plays the drive, and loads every imported map.

**Optimization, in a session the user starts.** The change-driven passes wait in [optimization](.notes/optimization.md), with what the first session measured.

## 1. Zero Hour's units drive

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`. It needs the turn rate, the load's rules and the selection to play.

### U6. Unit types move and have bodies

(`import`; Units that drive D1, D4, Data.) The import reads `Locomotor.ini` and each object's `Locomotor = SET_NORMAL`, its `KindOf` and its `Geometry`; each unit type gains `move_speed` and `turn_rate` from a locomotor of `GROUND`, and its body by D4's rule. The mode's manifest gains `stats`, `orders` and `navigation`, and `max_move_speed` the largest speed it writes. Check: fixture objects of a ground locomotor, an air one, a box structure, a small structure, a cylinder structure and no geometry give the unit types computed by hand.

### U7. Cliffs are walls

(`import`; Units that drive D3.) Each map's `[navigation]` gains `cell = 10` and its walls: its cliff cells, the bits of version 7 and 8 or the heights' rule of version 6, grown by one ring and merged into rectangles row by row, on the engine's axes. Check: a fixture map's cliffs, grown and merged, give the walls computed by hand; the Zero Hour check loads every imported map in the imported mode, and pins Tournament Desert's 286 walls.

### U8. The skirmish's start

(`import`, the rules package; Units that drive D6.) The import reads `PlayerTemplate.ini`, tags each `Player_<n>_Start` and `Player_<n>_Rally` waypoint `start`, and gives the mode `[navigation] spawn_tags = ["start"]` and the `[params]` of America's start; the rules package's mode script spawns, for the player at start position `n`, the building at `Player_<n>_Start` and each starting unit at `Player_<n>_Rally`, or at the building's point moved `start_offset` along `+z`, owned by the player. Check: the import's fixture install's params and tags; the Zero Hour check finds the command center at `Player_1_Start` and the dozer clear of its box, owned by the player; the dozer, ordered to `Player_2_Start`, arrives within a bound of ticks the check pins and logs, and never stands in a wall's cell; the user drives it in the client.

### U9. The Zero Hour check compares fingerprints and plays the parity suite

(`checks/zero-hour-check`, the parity oracle; Zero Hour D5, D7, D8.) The Zero Hour check compares the fingerprint of its import with the one the other OS gave, and plays the parity suite against the oracle's frames, as design 12 D7 says. It waits for the oracle, the fork of GeneralsX that writes each logic frame. Check: a run that names no install fails; a run on Linux and one on macOS give one fingerprint; the suite's replays meet the measure.
