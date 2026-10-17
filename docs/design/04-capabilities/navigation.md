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

- **One command, one route.** A move of many units is one group, and the group plans one long route for each layer, for its widest walker, which each unit joins at the last waypoint its own straight line reaches; a group costs one search a layer, about a tenth of its units one by one.
- **Formation only away from the group.** Each unit keeps its offset from the group's center when the goal lies outside their box, and goes to the goal itself when it lies inside, as StarCraft II keeps a selection's formation only for a move away from it; a spawn group's wave shares its route alike.

## Waypoint paths

- **Paths as lanes and patrols.** A unit walks a map's path in its spawn's direction while it has no other order, routing round the structures between waypoints.

## Forced movement

- **Dashes and knock backs pass through units,** as in League of Legends and Dota 2, and end before a static body; a dash crosses walls, as League of Legends' dashes do, and lands on the nearest cell it may stand in.
- **A dash follows its target,** as Lee Sin follows his mark, until their bodies touch.
- **Only a teleport disjoints,** as Blink does in Dota 2; a dash and a knock back do not, as Force Staff does not.
- **A forced move interrupts and replaces.** A moved unit takes no step and starts no action; a new forced move replaces the one under way.
- **A forced move is state,** and a client continues one it learned of.

## Map checks

- **A map that loads can be walked at its start.** The load checks, for each layer's widest walker, that each path's waypoints reach each other and that each marker of the mode's spawn tags is a place it may stand, using the regions the match uses.
- **Placed obstacles may overlap.** Placed static bodies may overlap walls and each other, as they block alike: Zero Hour's maps hold thousands of such overlaps and the original has no rule against them. A placement at run time still refuses an overlap ([Production](production.md#placement)).

## Collision

- **Collision reads the walkers alone.** The Collide stage reads only the units that can walk into its colliders, and a contact of a walker with a static body keeps that body as the static index holds it, as the static bodies never move, so the stage costs the walkers, not every body. Refused: every body read into the colliders each tick, and a search of them for the body each static contact names.
- **Pairs part by exact steps.** Each overlapping pair on one layer parts along the line between them, pair by pair in stable-id order, each coordinate rounded away from zero.
- **A standing body blocks.** A walker that runs into a unit that stands takes the whole overlap, so a standing unit's body blocks; a unit that cannot walk leaves all of it to the other.
- **Gatherers in their loop do not collide** ([Production](production.md#gathering)).
