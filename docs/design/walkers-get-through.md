# Walkers get through

A slice's proposal ([Documents](../AGENTS.md#documents)): its decisions fold into
[Navigation](04-capabilities/navigation.md) when it ends, and this document goes.

At its end, a player who sends a dozer past a column of idle dozers in a base sees the column
drive ahead out of its way, and no unit of a base start stands for good with a destination: each
walker either gets where it goes or stops within seconds. The facts are in
[Crowd movement](../research/crowd-movement.md).

## Decisions

- **A walker progresses only when it comes nearer its waypoint.** A tick counts toward being
  stuck unless the walker stands at least half its step nearer its waypoint than it ever stood,
  or passed a waypoint; a waypoint taken fresh, as a walk starts or past a waypoint, counts
  nothing, and a detour, a short route or a new route to a waypoint of their own in place of one
  not passed counts on. As 0 A.D.'s `CCmpUnitMotion` counts a failed movement each turn its move
  is obstructed or a push takes it away from where it moves, never escalates one that moves a
  little, and keeps the count across new paths. Refused: a move of half a step in any way, under
  which a hacker pushed back each tick by a crawler's personal space stood 0.77 m short of its
  goal for 6,240 ticks; a count a detour resets, under which a Red Guard took a detour it could
  not walk every 5 ticks and never asked a new route; a move of half a step along its leg each
  tick, under which a walker slowed in a crowd counted as stuck and the bases play's stalls rose
  from 332 to 1,065.
- **A walker stuck by a crowded goal has arrived.** A walker stuck where its route's last
  waypoint and itself lie within the personal space a unit that stands shares with it, as
  collision measures it, has arrived there, not short, as near as collision lets it come, as the
  A* Pathfinding Project stops a unit at a crowded destination and Zero Hour's locomotor ends a
  move within its close-enough distance. Refused: arrival within its radius of the waypoint,
  under which a dozer stopped 41 m short of its building site and the attack play never ended;
  5 asks of 1 s each, which leave it jittering by its goal for 5 s.
- **A spawn stands where it can walk.** The pathing grid takes a box's body as the box spawns,
  whether it overlaps a walker or not, and a unit a producer with neither an exit nor a rally
  point spawns stands at the center of the open cell nearest the producer, as a trained unit
  stands at the free cell nearest its exit and a teleport finds its cell
  (04-capabilities/production.md, A producer may name its exit and rally). Refused: a box that
  joins the grid from the next tick, under which a supply center's truck, sent as the center
  spawned, took the cell under the center as open, and collision pushed it out into a pocket of
  its base whose ways out are narrower than its body, which it never left; the producer's own
  place; a test of `room_near`'s places against the grid, as the truck was a spawn of
  production, and `room_near` keeps a gap clear of each body.
- **A stuck walker's route walls the units of other teams that stand.** The route a stuck walker
  asks treats the cells a unit of another team that stands holds as walls, as it does those a
  friend hemmed in holds, as OpenRA plans a blocked move again with every actor blocking and Zero
  Hour's pathfinder blocks a non-ally that stands and cannot be crushed (`enemyFixed`); its first
  route still prices them at nothing, so an attack goes to the units it fights. Refused: units of
  other teams priced on every route, which the design refused before, as an attack's route then
  goes round the units it should fight; neutrals alone, which leaves the same jam against an enemy
  that stands; friends that are busy walled, under which a 3v3 hero farmed below level 6; no
  walls, under which China's supply trucks planned through a civilian taxi for 16,890 ticks.
- **A route in sight is a line.** A route whose goal the walker may stand on and sees, by the
  exact test of the static bodies, the walls and the priced cells a route's smoothing makes, is
  the straight line to it, before any search of cells, as a route's smoothing keeps such a line.
  Refused: the cells alone, under which a goal seen past two posts that meet at a corner, which
  no step of cells passes, is out of reach, and every route in sight pays a search; the Zero Hour
  check's bases play ends in tick 11,012 with 323 stalls with the line, against 11,801 and 360
  without.
- **A friend yields through the friends that yield.** A friend's search for a place clear of the
  walker's way passes through the idle friends the walker may move, as cells it may cross and not
  as blockers, and may end on one; each it would pass or end on steps aside in turn for the
  friend's own walk, one step deeper, 2 deep at most, so a column in a corridor drives ahead,
  its front first, as Zero Hour's `moveAllies` moves the allies on an ally's way to its place and
  OpenRA's nudge moves an idle neighbour when no cell is free. Refused: friends as blockers of
  the search, under which a dozer column in a corridor was hemmed in and the dozer behind it
  arrived short; a push of idle friends by the walker, as StarCraft II pushes idle units, which a
  unit that drives does not take (04-capabilities/navigation.md, A unit that drives yields by
  driving).

## Checks

- A walker bound for a goal within a standing unit's personal space arrives once it is stuck
  there, and one whose goal a stuck unit holds stops after its 5 asks.
- A walker that takes detours it cannot walk asks its route again after 1 s.
- A spawn of a producer with neither an exit nor a rally point stands outside the producer's
  body, at the open cell nearest it.
- A stuck walker routes round a unit of another team that stands on its way.
- A goal seen past two posts that meet at a corner is the line to it, which no step of cells
  passes.
- A wide walker behind a column of two wide idle friends in a corridor with no side room arrives,
  the column driving ahead of it.
- The Zero Hour check's bases play has no walker that stands with a destination for 1,000 ticks,
  and its stalls and ends are named in each batch's commit.
