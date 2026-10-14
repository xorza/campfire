# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

**Slice 5, units that drive.** The user starts Tournament Desert in the client, finds America's command center and dozer at their start, selects the dozer, and drives it round the cliffs to the other start, at its speed and turn rate, on the drawn ground. The design is [Units that drive](docs/design/17-units-that-drive.md); the facts are [Zero Hour research](docs/research/zero-hour.md#movement). The Zero Hour check plays the drive, and loads every imported map.

## 1. Walkers turn, and imported maps load

`values`, `units`, `navigation`, `mode`, the test packages; `client`: `view`. Before the import, which writes the stat, the tags and the walls.

### U3. Walkers face their way, at their turn rate

(`navigation`, `units`, `values`, `client`; Units that drive D2.) `EngineStat` gains `TurnRate`, `turn_rate`. `Facing` becomes a part navigation writes in place, sent and predicted as `Position` is: a walker with a way to go turns toward its next step's way, the shorter way round, counter-clockwise on a tie, by at most its turn rate over the tick rate, rounded once, and steps only when it then faces within 11.25° of the way; a walker of no turn rate faces the way at once; forced moves and collision turn none. The client turns a unit's drawing as its facing changes, glided between ticks. Check: a walker of 90° a second at 30 ticks a second turns 3° a tick, and on a turn of 90° stands 26 ticks and steps on the 27th, its facing then 9° from the way, each tick's facing and place computed by hand; a turn of 180° goes counter-clockwise; a walker of none faces its way on its first step; the client predicts the facing with no correction; the goldens move once, as the test content's walkers now turn, and their new hashes are reviewed.

### U4. The load checks what a mode spawns at

(`mode`, `navigation`, the test packages; Units that drive D5.) The map load accepts placed static bodies that overlap walls and each other, and still refuses a box past the bounds; mode data gains `[navigation] spawn_tags`, resolved at the load, and the load checks only the markers of those tags; the lane and proving modes name theirs. Design 08's load rules and navigation's Map checks say so. Check: a fixture map of a box over a wall and of two boxes that overlap loads; a box past the bounds does not; a marker of a spawn tag in a wall's margin does not, one of another tag does; the test modes load and play as before.

## 2. The selection

`client`: `orders`, `pointer`, `view`.

### U5. Select and order a group

(`client`; Units that drive D7.) The client gains a selection of the player's own units: a left click on one selects it alone, a left drag the units whose ground point lies in its rectangle, and a click or a drag that meets none leaves the selection as it is; a right click sends the selection one group order, an attack on an enemy under the cursor, else a move to the ground point; the selection starts as the avatar. `view` draws a ring under each selected unit, of its radius. The ability and item keys act on the avatar as now. Check: each click, drag and right click on a fixture of units gives the selection and the order computed by hand; the proving mode plays as now.

## 3. Zero Hour's units drive

`import`: `zero_hour`; `source/packages/zero-hour`; `checks/zero-hour-check`. Last, as it needs the turn rate, the load's rules and the selection to play.

### U6. Unit types move and have bodies

(`import`; Units that drive D1, D4, Data.) The import reads `Locomotor.ini` and each object's `Locomotor = SET_NORMAL`, its `KindOf` and its `Geometry`; each unit type gains `move_speed` and `turn_rate` from a locomotor of `GROUND`, and its body by D4's rule. The mode's manifest gains `stats`, `orders` and `navigation`, and `max_move_speed` the largest speed it writes. Check: fixture objects of a ground locomotor, an air one, a box structure, a small structure, a cylinder structure and no geometry give the unit types computed by hand.

### U7. Cliffs are walls

(`import`; Units that drive D3.) Each map's `[navigation]` gains `cell = 10` and its walls: its cliff cells, the bits of version 7 and 8 or the heights' rule of version 6, grown by one ring and merged into rectangles row by row, on the engine's axes. Check: a fixture map's cliffs, grown and merged, give the walls computed by hand; the Zero Hour check loads every imported map in the imported mode, and pins Tournament Desert's 286 walls.

### U8. The skirmish's start

(`import`, the rules package; Units that drive D6.) The import reads `PlayerTemplate.ini`, tags each `Player_<n>_Start` and `Player_<n>_Rally` waypoint `start`, and gives the mode `[navigation] spawn_tags = ["start"]` and the `[params]` of America's start; the rules package's mode script spawns, for the player at start position `n`, the building at `Player_<n>_Start` and each starting unit at `Player_<n>_Rally`, or at the building's point moved `start_offset` along `+z`, owned by the player. Check: the import's fixture install's params and tags; the Zero Hour check finds the command center at `Player_1_Start` and the dozer clear of its box, owned by the player; the dozer, ordered to `Player_2_Start`, arrives within a bound of ticks the check pins and logs, and never stands in a wall's cell; the user drives it in the client.

## Foundations


