# Navigation

## Mechanism

- **Routes, paths, forced movement and collision on layers.** Navigation moves units on the layers of the core's space ([Space and map](00-overview.md#space-and-map)).
- **Backends behind one interface.** Queries take world positions, a route is a list of waypoints and obstacles are shapes, so every movement backend works with every pathfinding backend: grid A* and local steering now, a navmesh and flow fields later, and an imported game's own pathfinder and locomotors as backends for its parity.
- **Each layer is its own world.** Each layer has its own pathing grid, static bodies and collision, and a unit's layer is a tag, so a filter selects air or ground.

## Movement

- **Three kinds of avoidance, as Dota 2's.** A long route around what never moves, a short route around units in the way, and collision as the last guard, all exact fixed point, in stable-id order, under a fixed work limit a tick.
- **Regions as 0 A.D.'s hierarchical pathfinder.** For each kind of walker, chunks of 64 × 64 cells split into regions numbered as reachable sets, so an unreachable goal turns into the nearest reachable cell before any search spreads, as 0 A.D.'s `MakeGoalReachable`; a change of static bodies builds again only the chunks it changed.
- **One work limit for every route.** Long and short routes share a limit a tick, as many units of work as the grid has cells; long routes go first, and the plan that meets the limit finishes while the rest wait, so the worst tick stays a small share of the budget.
- **A walker faces its leg and turns at its turn rate.** Each tick a walker turns toward its leg's way the shorter way round, at most its `turn_rate` over the tick rate, and steps only when it faces within 11.25° of the way, as Dota 2 lets a unit move within 0.2 radians; the way holds for a whole leg whatever each step rounds. A forced move and collision turn no unit.
- **Walks through a contract of `units`.** A capability asks for a walk by `Destination`, never by navigation's own state, and reads `ArrivedShort` to know a walk ended short, so production and abilities walk units with no dependency on navigation.
- **A route ends short, not never.** A unit that arrives short of its goal drops its destination and keeps its route, so the same goal plans nothing again until a static body goes.
- **Head-on walkers pass on the right.** A walker kept back for 150 ms treats those it touches as offset to its left, so two that meet head on pass.
- **A client predicts its own routes** on the same grid, with no correction.

## Groups

- **One command, one route.** A move of many units is one group, and the group plans one long route for each layer, for its widest walker, which each unit joins at the last waypoint its own straight line reaches; a group costs one search a layer, about a tenth of its units one by one. A route that stops short of the group's goal serves only a unit of the walker it was searched for that has that goal: a narrower one, or one of its own goal, plans its own, from the tick's route budget, as one that sees no waypoint does.
- **Formation only away from the group.** Each unit keeps its offset from the group's center when the goal lies outside their box, and goes to the goal itself when it lies inside, as StarCraft II keeps a selection's formation only for a move away from it; a spawn group's wave shares its route alike.

## Waypoint paths

- **Paths as lanes and patrols.** A unit walks a map's path in its spawn's direction while it has no other order, routing round the structures between waypoints; past the last waypoint it stays, goes on to the first, or turns back, as the path's `walk` says.
- **A path says how it is walked.** The walk is the path's map data, as a Zero Hour map's waypoint path holds its own links and its `biDirectional` flag, so a lane and a patrol differ by their paths. Refused: a walk for each walker, which each spawn call and placement would repeat.
- **Navigation follows paths.** Path walkers walk in navigation, before orders' chase, so a path's walk, its end and its hook are one crate's; orders only tells a walker to leave its path or rejoin it.
- **A path's end runs a hook.** A walker that reaches its last waypoint runs `on_path_end`, queued on the server by stable id in its tick; a predicting client walks alike and runs no hook.

## Forced movement

- **Dashes and knock backs pass through units,** as in League of Legends and Dota 2, and end before a static body; a dash crosses walls, as League of Legends' dashes do, and lands on the nearest cell it may stand in.
- **A dash follows its target,** as Lee Sin follows his mark, until their bodies touch.
- **Only a teleport disjoints,** as Blink does in Dota 2; a dash and a knock back do not, as Force Staff does not.
- **A forced move interrupts and replaces.** A moved unit takes no step and starts no action; a new forced move replaces the one under way.
- **A forced move is state,** and a client continues one it learned of.

## Map checks

- **A map that loads can be walked at its start.** The load checks, for each layer's widest walker, that each path's waypoints reach each other and that each marker of the mode's spawn tags is a place it may stand, using the regions the match uses.
- **The walk check builds its own grid.** The load builds a grid for each layer's widest walker with the placed bodies and drops it; the match builds one for every walker kind, which its static tracking fills with the match's own bodies, as Zero Hour builds its pathfinder at each map load (`Pathfinder::newMap`). Refused: one grid kept as a book and cloned by each match, which moves the check from the package load to the match start, so a map that cannot be walked loads.
- **Placed obstacles may overlap.** Placed static bodies may overlap walls and each other, as they block alike: Zero Hour's maps hold thousands of such overlaps and the original has no rule against them. A placement at run time still refuses an overlap ([Production](production.md#placement)).
- **Walker classes and a route budget, planned.** A mode declares its walker classes, so the clearance and region sets are one for each class, not one for each distinct radius, once a Zero Hour import's count of distinct walker radii is measured; and a route budget for each tick takes the place of the map's cell count as the route work limit.

## Room

- **Every body is tested for room.** `spawn_unit` refuses a unit whose body overlaps a wall, a static body, or any body the same call spawns, a circle and a box alike, a walker's among them ([Test matches](../../research/test-matches.md)). Refused: the test of boxes alone, under which a cylinder building spawns over another and a call's walkers spawn on one point.
- **A script finds room near a point.** `room_near(type, position, gap, steps)` gives the nearest point within `steps` gaps of `position`, at most 128, on a grid of `gap` meters round it, by distance and then by angle from `+x` as a facing turns, each compared exactly, where a unit of `type` has room as `spawn_unit` tests it, `gap` meters clear of each body and wall, a box grown at each side; none when no point within its steps has room, as StarCraft II's search takes its range. It takes the nearest point, as StarCraft II's `UnitTypePlacementFromPoint` and Age of Empires II's `find_closest` do, so a base stands packed round its start and the same in every match; its range counts grid steps, as a script call takes one number of meters at most, so its bound is exact. Refused: a cell drawn at random from an annulus, as OpenRA's support actors take, under which a test does not repeat; positions in data for each map and faction, made again for each.

## Collision

- **Collision reads the walkers alone.** The Collide stage reads only the units that can walk into its colliders, and a contact of a walker with a static body keeps that body as the static index holds it, as the static bodies never move, so the stage costs the walkers, not every body. Refused: every body read into the colliders each tick, and a search of them for the body each static contact names.
- **Pairs part by exact steps.** Each overlapping pair on one layer parts along the line between them, pair by pair in stable-id order, each coordinate rounded away from zero.
- **A standing body blocks.** A walker that runs into a unit that stands takes the whole overlap, so a standing unit's body blocks; a unit that cannot walk leaves all of it to the other. An idle friend in a stuck walker's way steps aside (An idle friend steps aside).
- **An idle friend steps aside.** A walker stuck for `STUCK_MS` makes each unit of its team it touches that stands idle, with no destination, nothing under way, no target and no work that holds it, step off its way, on the side it stands, or to the walker's left from the way itself, as walkers pass on the right, until it clears the walker's body, at the nearest place it may stand there; a builder at its build stays, as production shows in `AtWork`, and so does an enemy. It is the server's: a predicting client takes the step from the server. Zero Hour's `AIUpdateInterface` tells an idle unit in the way to `aiMoveAwayFromUnit`, a busy one not, and its pathfinder moves allies off a new path; OpenRA's `Mobile` queues a `Nudge` on an idle friendly actor that blocks a move ([Test matches](../../research/test-matches.md)). Refused: a standing body that blocks for good, under which the trained units at a rally hold back every walker behind them; a spread of trained units round their rally alone, which leaves every other idle crowd in the way.
- **A walker never stays in a static body.** A walker its static contacts leave overlapping a static body, as one wedged between two closer than its body is wide, which each contact pushes into the other, stands at the nearest place it may stand on the pathing grid, as a static body that spawns on a walker moves it. Refused: the contacts alone, under which such a walker swings between the bodies each tick and stands in blocked cells for good ([Test matches](../../research/test-matches.md)).
- **A route leaves a blocked start.** A long route from a cell no open cell borders, as a walker pushed into a gap narrower than its clearance stands in, leaves from the open cell nearest it, as one beside it does. Refused: a route of the walker's own place, which it never walks out of.
- **Gatherers in their loop do not collide** ([Production](production.md#gathering)).
